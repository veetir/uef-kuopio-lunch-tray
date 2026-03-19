//! Text measurement and wrapping helpers for the popup layout pipeline.

use super::*;

pub(super) fn measure_lines_layout(
    hdc: HDC,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    lines: &[Line],
    wrap_content_width: i32,
) -> LineLayoutMetrics {
    let wrap_width = wrap_content_width.max(40);
    let bullet_width = text_width_with_font(hdc, normal_font, BULLET_PREFIX);
    let main_wrap_width = (wrap_width - bullet_width).max(24);
    let mut required_content_width = 0;
    let mut wrapped_line_count = 0usize;

    for line in lines {
        match line {
            Line::Heading(text) => {
                let width = text_width_with_font(hdc, bold_font, text);
                required_content_width = required_content_width.max(width);
                let rows = wrapped_line_count_for_text(hdc, bold_font, text, wrap_width);
                wrapped_line_count += rows.max(1);
            }
            Line::Text(text) => {
                let width = text_width_with_font(hdc, normal_font, text);
                required_content_width = required_content_width.max(width);
                let rows = wrapped_line_count_for_text(hdc, normal_font, text, wrap_width);
                wrapped_line_count += rows.max(1);
            }
            Line::MenuItem {
                main,
                suffix_segments,
            } => {
                let styled_width = text_with_suffix_width(
                    hdc,
                    normal_font,
                    small_font,
                    small_bold_font,
                    main,
                    suffix_segments,
                    bullet_width,
                );
                required_content_width = required_content_width.max(styled_width);
                if styled_width <= wrap_width {
                    wrapped_line_count += 1;
                } else {
                    let main_rows =
                        wrapped_line_count_for_text(hdc, normal_font, main, main_wrap_width).max(1);
                    wrapped_line_count += main_rows;
                    if !suffix_segments.is_empty() {
                        let suffix_plain = flatten_suffix_segments(suffix_segments);
                        if !suffix_plain.is_empty() {
                            let suffix_rows = wrapped_line_count_for_text(
                                hdc,
                                small_font,
                                &suffix_plain,
                                main_wrap_width,
                            )
                            .max(1);
                            wrapped_line_count += suffix_rows;
                        }
                    }
                }
            }
            Line::Spacer => {
                wrapped_line_count += 1;
            }
        }
    }

    LineLayoutMetrics {
        required_content_width,
        wrapped_line_count,
    }
}

fn wrapped_line_count_for_text(hdc: HDC, font: HFONT, text: &str, max_width: i32) -> usize {
    let wrapped = wrap_text_to_width_with_font(hdc, font, text, max_width);
    wrapped.len()
}

pub(in crate::popup) fn wrap_text_to_width_with_font_rows(
    hdc: HDC,
    font: HFONT,
    text: &str,
    max_width: i32,
) -> Vec<WrappedRow> {
    unsafe {
        let old = SelectObject(hdc, font);
        let wrapped = wrap_text_to_width_rows(hdc, text, max_width);
        SelectObject(hdc, old);
        wrapped
    }
}

pub(in crate::popup) fn wrap_text_to_width_with_font(
    hdc: HDC,
    font: HFONT,
    text: &str,
    max_width: i32,
) -> Vec<String> {
    wrap_text_to_width_with_font_rows(hdc, font, text, max_width)
        .into_iter()
        .map(|row| row.text)
        .collect()
}

pub(in crate::popup) fn wrap_text_to_width(hdc: HDC, text: &str, max_width: i32) -> Vec<String> {
    wrap_text_to_width_rows(hdc, text, max_width)
        .into_iter()
        .map(|row| row.text)
        .collect()
}

fn wrap_text_to_width_rows(hdc: HDC, text: &str, max_width: i32) -> Vec<WrappedRow> {
    let clean = normalize_text(text);
    if clean.is_empty() {
        return Vec::new();
    }
    let limit = max_width.max(16);
    if text_width(hdc, &clean) <= limit {
        return vec![WrappedRow {
            text: clean.clone(),
            start: 0,
            end: clean.len(),
        }];
    }

    let words = word_bounds(&clean);
    if words.is_empty() {
        return vec![WrappedRow {
            text: clean.clone(),
            start: 0,
            end: clean.len(),
        }];
    }

    let mut rows: Vec<WrappedRow> = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut current_end: usize = 0;
    for word in words {
        let candidate_range = match current_start {
            Some(start) => start..word.end,
            None => word.start..word.end,
        };
        let candidate_text = &clean[candidate_range.clone()];
        if text_width(hdc, candidate_text) <= limit {
            if current_start.is_none() {
                current_start = Some(word.start);
            }
            current_end = word.end;
            continue;
        }

        if let Some(start) = current_start {
            let text = clean[start..current_end].to_string();
            rows.push(WrappedRow {
                text,
                start,
                end: current_end,
            });
            current_start = None;
            current_end = 0;
        }

        let single_word = &clean[word.start..word.end];
        if text_width(hdc, single_word) <= limit {
            current_start = Some(word.start);
            current_end = word.end;
        } else {
            rows.extend(split_long_token_to_width_rows(
                hdc, &clean, word.start, word.end, limit,
            ));
        }
    }

    if let Some(start) = current_start {
        rows.push(WrappedRow {
            text: clean[start..current_end].to_string(),
            start,
            end: current_end,
        });
    }

    if rows.is_empty() {
        rows.push(WrappedRow {
            text: clean.clone(),
            start: 0,
            end: clean.len(),
        });
    }
    rows
}

#[derive(Debug, Clone, Copy)]
struct WordBounds {
    start: usize,
    end: usize,
}

fn word_bounds(text: &str) -> Vec<WordBounds> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(s) = start.take() {
                out.push(WordBounds { start: s, end: idx });
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    if let Some(s) = start {
        out.push(WordBounds {
            start: s,
            end: text.len(),
        });
    }
    out
}

fn split_long_token_to_width_rows(
    hdc: HDC,
    full_text: &str,
    start: usize,
    end: usize,
    max_width: i32,
) -> Vec<WrappedRow> {
    let mut rows = Vec::new();
    let token = &full_text[start..end];
    let mut current = String::new();
    let mut current_start = start;
    let mut current_end = start;

    for (offset, ch) in token.char_indices() {
        let ch_len = ch.len_utf8();
        let mut candidate = current.clone();
        candidate.push(ch);
        if !current.is_empty() && text_width(hdc, &candidate) > max_width {
            rows.push(WrappedRow {
                text: current.clone(),
                start: current_start,
                end: current_end,
            });
            current.clear();
            current_start = start + offset;
        }
        current.push(ch);
        current_end = start + offset + ch_len;
    }
    if !current.is_empty() {
        rows.push(WrappedRow {
            text: current,
            start: current_start,
            end: current_end,
        });
    }
    if rows.is_empty() {
        rows.push(WrappedRow {
            text: token.to_string(),
            start,
            end,
        });
    }
    rows
}

pub(in crate::popup) fn text_width_with_font(hdc: HDC, font: HFONT, text: &str) -> i32 {
    unsafe {
        let old = SelectObject(hdc, font);
        let width = text_width(hdc, text);
        SelectObject(hdc, old);
        width
    }
}

pub(in crate::popup) fn text_with_suffix_width(
    hdc: HDC,
    normal_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    main: &str,
    segments: &[(String, bool)],
    bullet_width: i32,
) -> i32 {
    let main_width = text_width_with_font(hdc, normal_font, main);
    if segments.is_empty() {
        return bullet_width + main_width;
    }

    let mut suffix_width = 0;
    for (segment, bold) in segments {
        let font = if *bold { small_bold_font } else { small_font };
        suffix_width += text_width_with_font(hdc, font, segment);
    }
    bullet_width + main_width + suffix_width + 4
}

pub(in crate::popup) fn flatten_suffix_segments(segments: &[(String, bool)]) -> String {
    let mut out = String::new();
    for (segment, _) in segments {
        out.push_str(segment);
    }
    normalize_text(&out)
}

pub(in crate::popup) fn text_metrics(hdc: HDC, font: HFONT) -> TEXTMETRICW {
    unsafe {
        let old = SelectObject(hdc, font);
        let mut metrics = TEXTMETRICW::default();
        GetTextMetricsW(hdc, &mut metrics);
        SelectObject(hdc, old);
        metrics
    }
}

pub(in crate::popup) fn text_width(hdc: HDC, text: &str) -> i32 {
    let wide = to_wstring(text);
    unsafe {
        let mut size = windows::Win32::Foundation::SIZE::default();
        if wide.len() > 1 {
            let slice = &wide[..wide.len() - 1];
            let _ = GetTextExtentPoint32W(hdc, slice, &mut size);
        }
        size.cx
    }
}

#[cfg(test)]
mod tests {
    use super::{flatten_suffix_segments, word_bounds};

    #[test]
    fn word_bounds_splits_on_whitespace_runs() {
        let bounds = word_bounds("tofu   curry\nwith rice");
        let pairs: Vec<(usize, usize)> = bounds.into_iter().map(|b| (b.start, b.end)).collect();
        assert_eq!(pairs, vec![(0, 4), (7, 12), (13, 17), (18, 22)]);
    }

    #[test]
    fn flatten_suffix_segments_normalizes_spacing() {
        let segments = vec![
            ("(A,".to_string(), false),
            ("  L, ".to_string(), true),
            ("Veg )".to_string(), false),
        ];
        assert_eq!(flatten_suffix_segments(&segments), "(A, L, Veg )");
    }
}
