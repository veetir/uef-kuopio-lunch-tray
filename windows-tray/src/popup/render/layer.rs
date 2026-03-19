//! High-level paint orchestration and line-by-line content rendering.

use super::text::{
    add_selectable_row, draw_header_button, draw_main_segments, draw_selection_bg_for_row,
    draw_text_line, draw_text_segments, favorite_match_ranges, fit_text_to_width, segments_for_row,
    text_segments_width, LinePlacement, RowBounds, RowCaptureContext, SegmentColors, SegmentFonts,
    SegmentStyle, SelectionOverlay,
};
use super::*;
pub(in crate::popup) fn paint_popup(hwnd: HWND, state: &AppState) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let paint_hdc = BeginPaint(hwnd, &mut ps);
        if paint_hdc.0 == 0 {
            return;
        }

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        let buffer_dc = CreateCompatibleDC(paint_hdc);
        if buffer_dc.0 == 0 {
            EndPaint(hwnd, &ps);
            return;
        }
        let buffer_bitmap = CreateCompatibleBitmap(paint_hdc, width, height);
        if buffer_bitmap.0 == 0 {
            DeleteDC(buffer_dc);
            EndPaint(hwnd, &ps);
            return;
        }
        let old_bitmap = SelectObject(buffer_dc, buffer_bitmap);
        let hdc = buffer_dc;
        let palette = theme_palette(&state.settings.theme);
        let brush = CreateSolidBrush(palette.bg_color);
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
        SetBkMode(hdc, TRANSPARENT);

        let scale = popup_scale(&state.settings);
        let (normal_font, bold_font, small_font, small_bold_font) =
            create_fonts(hdc, &state.settings.theme, scale.factor);
        let _old_font = SelectObject(hdc, normal_font);

        let metrics = text_metrics(hdc, normal_font);
        let line_height = metrics.tmHeight as i32 + scale.line_gap;
        let content_width = (width - scale.padding_x * 2).max(40);
        let animation = current_animation_frame(hwnd);
        let favorites = current_favorites_snapshot();

        let header_rect = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.top + scale.header_height,
        };
        let header_brush = CreateSolidBrush(palette.header_bg_color);
        FillRect(hdc, &header_rect, header_brush);
        DeleteObject(header_brush);

        let layout = header_layout(width, &scale);
        let pressed_button = pressed_header_button(hwnd);
        draw_header_button(
            hdc,
            &layout.prev,
            "<",
            palette.button_bg_color,
            palette.body_text_color,
            normal_font,
            pressed_button == Some(HeaderButtonAction::Prev),
        );
        draw_header_button(
            hdc,
            &layout.next,
            ">",
            palette.button_bg_color,
            palette.body_text_color,
            normal_font,
            pressed_button == Some(HeaderButtonAction::Next),
        );
        draw_header_button(
            hdc,
            &layout.close,
            "X",
            palette.button_bg_color,
            palette.body_text_color,
            normal_font,
            false,
        );

        let divider_rect = RECT {
            left: rect.left,
            top: header_rect.bottom - 1,
            right: rect.right,
            bottom: header_rect.bottom,
        };
        let divider_brush = CreateSolidBrush(palette.divider_color);
        FillRect(hdc, &divider_rect, divider_brush);
        DeleteObject(divider_brush);

        if let Some(frame) = animation {
            clear_selection_layout(hwnd);
            match frame {
                PopupAnimationFrame::Open {
                    lines,
                    title,
                    progress,
                } => {
                    let y_offset =
                        ((1.0 - progress) * scale.switch_offset_px as f32).round() as i32;
                    let layer_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, progress);
                    let layer_heading =
                        lerp_color(palette.bg_color, palette.heading_color, progress);
                    let layer_title =
                        lerp_color(palette.bg_color, palette.header_title_color, progress);
                    let layer_suffix = lerp_color(palette.bg_color, palette.suffix_color, progress);
                    let layer_suffix_highlight =
                        lerp_color(palette.bg_color, palette.suffix_highlight_color, progress);
                    let layer_favorites =
                        lerp_color(palette.bg_color, palette.favorite_highlight_color, progress);
                    draw_content_layer(
                        hdc,
                        &title,
                        &lines,
                        DrawLayerParams {
                            scale,
                            width,
                            content_width,
                            body_text_color: layer_body_text,
                            heading_color: layer_heading,
                            header_title_color: layer_title,
                            suffix_color: layer_suffix,
                            suffix_highlight_color: layer_suffix_highlight,
                            favorite_highlight_color: layer_favorites,
                            selection_bg_color: palette.selection_bg_color,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            favorites: &favorites,
                            selection: None,
                            capture: None,
                            y_offset,
                        },
                    );
                }
                PopupAnimationFrame::Close {
                    lines,
                    title,
                    progress,
                } => {
                    let y_offset = 0;
                    let layer_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, 1.0 - progress);
                    let layer_heading =
                        lerp_color(palette.bg_color, palette.heading_color, 1.0 - progress);
                    let layer_title =
                        lerp_color(palette.bg_color, palette.header_title_color, 1.0 - progress);
                    let layer_suffix =
                        lerp_color(palette.bg_color, palette.suffix_color, 1.0 - progress);
                    let layer_suffix_highlight = lerp_color(
                        palette.bg_color,
                        palette.suffix_highlight_color,
                        1.0 - progress,
                    );
                    let layer_favorites = lerp_color(
                        palette.bg_color,
                        palette.favorite_highlight_color,
                        1.0 - progress,
                    );
                    draw_content_layer(
                        hdc,
                        &title,
                        &lines,
                        DrawLayerParams {
                            scale,
                            width,
                            content_width,
                            body_text_color: layer_body_text,
                            heading_color: layer_heading,
                            header_title_color: layer_title,
                            suffix_color: layer_suffix,
                            suffix_highlight_color: layer_suffix_highlight,
                            favorite_highlight_color: layer_favorites,
                            selection_bg_color: palette.selection_bg_color,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            favorites: &favorites,
                            selection: None,
                            capture: None,
                            y_offset,
                        },
                    );
                }
                PopupAnimationFrame::Switch {
                    old_lines,
                    new_lines,
                    old_title,
                    new_title,
                    direction,
                    progress,
                } => {
                    let dir = if direction >= 0 { 1 } else { -1 };
                    let old_offset =
                        -dir * ((progress * scale.switch_offset_px as f32).round() as i32);
                    let new_offset =
                        dir * (((1.0 - progress) * scale.switch_offset_px as f32).round() as i32);
                    let old_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, 1.0 - progress);
                    let old_heading =
                        lerp_color(palette.bg_color, palette.heading_color, 1.0 - progress);
                    let old_title_color =
                        lerp_color(palette.bg_color, palette.header_title_color, 1.0 - progress);
                    let old_suffix =
                        lerp_color(palette.bg_color, palette.suffix_color, 1.0 - progress);
                    let old_suffix_highlight = lerp_color(
                        palette.bg_color,
                        palette.suffix_highlight_color,
                        1.0 - progress,
                    );
                    let old_favorites = lerp_color(
                        palette.bg_color,
                        palette.favorite_highlight_color,
                        1.0 - progress,
                    );
                    let new_body_text =
                        lerp_color(palette.bg_color, palette.body_text_color, progress);
                    let new_heading = lerp_color(palette.bg_color, palette.heading_color, progress);
                    let new_title_color =
                        lerp_color(palette.bg_color, palette.header_title_color, progress);
                    let new_suffix = lerp_color(palette.bg_color, palette.suffix_color, progress);
                    let new_suffix_highlight =
                        lerp_color(palette.bg_color, palette.suffix_highlight_color, progress);
                    let new_favorites =
                        lerp_color(palette.bg_color, palette.favorite_highlight_color, progress);
                    draw_content_layer(
                        hdc,
                        &old_title,
                        &old_lines,
                        DrawLayerParams {
                            scale,
                            width,
                            content_width,
                            body_text_color: old_body_text,
                            heading_color: old_heading,
                            header_title_color: old_title_color,
                            suffix_color: old_suffix,
                            suffix_highlight_color: old_suffix_highlight,
                            favorite_highlight_color: old_favorites,
                            selection_bg_color: palette.selection_bg_color,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            favorites: &favorites,
                            selection: None,
                            capture: None,
                            y_offset: old_offset,
                        },
                    );
                    draw_content_layer(
                        hdc,
                        &new_title,
                        &new_lines,
                        DrawLayerParams {
                            scale,
                            width,
                            content_width,
                            body_text_color: new_body_text,
                            heading_color: new_heading,
                            header_title_color: new_title_color,
                            suffix_color: new_suffix,
                            suffix_highlight_color: new_suffix_highlight,
                            favorite_highlight_color: new_favorites,
                            selection_bg_color: palette.selection_bg_color,
                            layout: &layout,
                            metrics: &metrics,
                            line_height,
                            normal_font,
                            bold_font,
                            small_font,
                            small_bold_font,
                            favorites: &favorites,
                            selection: None,
                            capture: None,
                            y_offset: new_offset,
                        },
                    );
                }
            }
        } else {
            let lines = build_lines(state);
            let title = header_title(state);
            let selection = current_selection_range(hwnd);
            let mut capture = DrawCapture {
                layout: SelectableLayout {
                    hwnd,
                    ..Default::default()
                },
            };
            draw_content_layer(
                hdc,
                &title,
                &lines,
                DrawLayerParams {
                    scale,
                    width,
                    content_width,
                    body_text_color: palette.body_text_color,
                    heading_color: palette.heading_color,
                    header_title_color: palette.header_title_color,
                    suffix_color: palette.suffix_color,
                    suffix_highlight_color: palette.suffix_highlight_color,
                    favorite_highlight_color: palette.favorite_highlight_color,
                    selection_bg_color: palette.selection_bg_color,
                    layout: &layout,
                    metrics: &metrics,
                    line_height,
                    normal_font,
                    bold_font,
                    small_font,
                    small_bold_font,
                    favorites: &favorites,
                    selection: selection.as_ref(),
                    capture: Some(&mut capture),
                    y_offset: 0,
                },
            );
            store_selection_layout(capture.layout);
        }

        SelectObject(hdc, _old_font);
        DeleteObject(normal_font);
        DeleteObject(bold_font);
        DeleteObject(small_font);
        DeleteObject(small_bold_font);
        let _ = BitBlt(paint_hdc, 0, 0, width, height, hdc, 0, 0, SRCCOPY);
        SelectObject(hdc, old_bitmap);
        DeleteObject(buffer_bitmap);
        DeleteDC(hdc);
        EndPaint(hwnd, &ps);
    }
}

struct DrawLayerParams<'a> {
    // Group render-only state so the content-layer draw path stays readable.
    scale: PopupScale,
    width: i32,
    content_width: i32,
    body_text_color: COLORREF,
    heading_color: COLORREF,
    header_title_color: COLORREF,
    suffix_color: COLORREF,
    suffix_highlight_color: COLORREF,
    favorite_highlight_color: COLORREF,
    selection_bg_color: COLORREF,
    layout: &'a HeaderLayout,
    metrics: &'a TEXTMETRICW,
    line_height: i32,
    normal_font: HFONT,
    bold_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    favorites: &'a FavoritesSnapshot,
    selection: Option<&'a SelectionRange>,
    capture: Option<&'a mut DrawCapture>,
    y_offset: i32,
}

fn draw_content_layer(hdc: HDC, title: &str, lines: &[Line], params: DrawLayerParams<'_>) {
    unsafe {
        SelectObject(hdc, params.bold_font);
        SetTextColor(hdc, params.header_title_color);
    }

    let full_title = normalize_text(title);
    let title_width = text_width(hdc, &full_title);
    let title_button_margin = scale_px(HEADER_TITLE_BUTTON_MARGIN, params.scale.factor);
    let min_title_x = params.layout.next.right + title_button_margin;
    let max_title_x =
        (params.layout.close.left - title_width - title_button_margin).max(min_title_x);
    let title_x = ((params.width - title_width) / 2).clamp(min_title_x, max_title_x);
    let title_y =
        ((params.scale.header_height - params.metrics.tmHeight) / 2 - 1) + params.y_offset;
    draw_text_line(hdc, &full_title, title_x, title_y);

    let bullet_width = text_width_with_font(hdc, params.normal_font, BULLET_PREFIX);
    let main_wrap_width = (params.content_width - bullet_width).max(24);

    let mut y = params.scale.header_height + params.scale.padding_y + params.y_offset;
    let mut capture = params.capture;
    for line in lines {
        match line {
            Line::Heading(text) => {
                unsafe {
                    SelectObject(hdc, params.bold_font);
                    SetTextColor(hdc, params.heading_color);
                }
                let wrapped = wrap_text_to_width(hdc, text, params.content_width);
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, params.scale.padding_x, y);
                        y += params.line_height;
                    }
                }
            }
            Line::Text(text) => {
                unsafe {
                    SelectObject(hdc, params.normal_font);
                    SetTextColor(hdc, params.body_text_color);
                }
                let wrapped = wrap_text_to_width(hdc, text, params.content_width);
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, params.scale.padding_x, y);
                        y += params.line_height;
                    }
                }
            }
            Line::MenuItem {
                main,
                suffix_segments,
            } => {
                unsafe {
                    SelectObject(hdc, params.normal_font);
                    SetTextColor(hdc, params.body_text_color);
                }
                let favorite_ranges = favorite_match_ranges(main, params.favorites);
                let styled_width = text_with_suffix_width(
                    hdc,
                    params.normal_font,
                    params.small_font,
                    params.small_bold_font,
                    main,
                    suffix_segments,
                    bullet_width,
                );
                let item_id = if let Some(ref mut draw_capture) = capture {
                    let id = draw_capture.layout.items.len();
                    draw_capture.layout.items.push(main.clone());
                    Some(id)
                } else {
                    None
                };
                let selected_item_range = item_id
                    .and_then(|id| params.selection.filter(|sel| sel.item_id == id).copied());

                if styled_width <= params.content_width {
                    let mut suffix_width = 0;
                    for (segment, bold) in suffix_segments {
                        let font = if *bold {
                            params.small_bold_font
                        } else {
                            params.small_font
                        };
                        unsafe {
                            SelectObject(hdc, font);
                        }
                        suffix_width += text_width(hdc, segment);
                    }
                    let max_main = (params.content_width - bullet_width - suffix_width - 4).max(24);
                    unsafe {
                        SelectObject(hdc, params.normal_font);
                        SetTextColor(hdc, params.body_text_color);
                    }
                    let clipped_main = fit_text_to_width(hdc, main, max_main);
                    let line_x = params.scale.padding_x + bullet_width;
                    let row = WrappedRow {
                        start: 0,
                        end: clipped_main.len(),
                        text: clipped_main.clone(),
                    };
                    let row_segments =
                        segments_for_row(&clipped_main, row.start, row.end, &favorite_ranges);
                    draw_text_line(hdc, BULLET_PREFIX, params.scale.padding_x, y);
                    if let Some(selection) = selected_item_range {
                        draw_selection_bg_for_row(
                            hdc,
                            &row,
                            RowBounds {
                                left: line_x,
                                top: y,
                                line_height: params.line_height,
                            },
                            SelectionOverlay {
                                start: selection.start,
                                end: selection.end,
                                bg_color: params.selection_bg_color,
                            },
                        );
                    }
                    draw_main_segments(
                        hdc,
                        &row_segments,
                        LinePlacement { x: line_x, y },
                        SegmentStyle {
                            fonts: SegmentFonts {
                                normal: params.normal_font,
                                bold: params.bold_font,
                            },
                            colors: SegmentColors {
                                normal: params.body_text_color,
                                highlight: params.favorite_highlight_color,
                            },
                        },
                    );
                    if let Some(ref mut draw_capture) = capture {
                        add_selectable_row(
                            &mut draw_capture.layout,
                            item_id.unwrap_or(0),
                            &row,
                            RowCaptureContext {
                                bounds: RowBounds {
                                    left: line_x,
                                    top: y,
                                    line_height: params.line_height,
                                },
                                hdc,
                                font: params.normal_font,
                            },
                        );
                    }
                    if !suffix_segments.is_empty() {
                        let main_width = text_segments_width(
                            hdc,
                            &row_segments,
                            params.normal_font,
                            params.bold_font,
                        );
                        let suffix_x = line_x + main_width + 4;
                        if suffix_x < (params.scale.padding_x + params.content_width) {
                            draw_text_segments(
                                hdc,
                                suffix_segments,
                                LinePlacement {
                                    x: suffix_x,
                                    y: y + 1,
                                },
                                SegmentStyle {
                                    fonts: SegmentFonts {
                                        normal: params.small_font,
                                        bold: params.small_bold_font,
                                    },
                                    colors: SegmentColors {
                                        normal: params.suffix_color,
                                        highlight: params.suffix_highlight_color,
                                    },
                                },
                            );
                        }
                    }
                    y += params.line_height;
                    continue;
                }

                let wrapped_main = wrap_text_to_width_with_font_rows(
                    hdc,
                    params.normal_font,
                    main,
                    main_wrap_width,
                );
                if wrapped_main.is_empty() {
                    y += params.line_height;
                } else {
                    for (idx, row) in wrapped_main.iter().enumerate() {
                        let line_x = params.scale.padding_x + bullet_width;
                        if idx == 0 {
                            draw_text_line(hdc, BULLET_PREFIX, params.scale.padding_x, y);
                        }
                        if let Some(selection) = selected_item_range {
                            draw_selection_bg_for_row(
                                hdc,
                                row,
                                RowBounds {
                                    left: line_x,
                                    top: y,
                                    line_height: params.line_height,
                                },
                                SelectionOverlay {
                                    start: selection.start,
                                    end: selection.end,
                                    bg_color: params.selection_bg_color,
                                },
                            );
                        }
                        let row_segments =
                            segments_for_row(main, row.start, row.end, &favorite_ranges);
                        draw_main_segments(
                            hdc,
                            &row_segments,
                            LinePlacement { x: line_x, y },
                            SegmentStyle {
                                fonts: SegmentFonts {
                                    normal: params.normal_font,
                                    bold: params.bold_font,
                                },
                                colors: SegmentColors {
                                    normal: params.body_text_color,
                                    highlight: params.favorite_highlight_color,
                                },
                            },
                        );
                        if let Some(ref mut draw_capture) = capture {
                            add_selectable_row(
                                &mut draw_capture.layout,
                                item_id.unwrap_or(0),
                                row,
                                RowCaptureContext {
                                    bounds: RowBounds {
                                        left: line_x,
                                        top: y,
                                        line_height: params.line_height,
                                    },
                                    hdc,
                                    font: params.normal_font,
                                },
                            );
                        }
                        y += params.line_height;
                    }
                }

                if !suffix_segments.is_empty() {
                    let suffix_plain = flatten_suffix_segments(suffix_segments);
                    if !suffix_plain.is_empty() {
                        let wrapped_suffix = wrap_text_to_width_with_font(
                            hdc,
                            params.small_font,
                            &suffix_plain,
                            params.content_width,
                        );
                        if wrapped_suffix.len() == 1 {
                            draw_text_segments(
                                hdc,
                                suffix_segments,
                                LinePlacement {
                                    x: params.scale.padding_x + bullet_width,
                                    y: y + 1,
                                },
                                SegmentStyle {
                                    fonts: SegmentFonts {
                                        normal: params.small_font,
                                        bold: params.small_bold_font,
                                    },
                                    colors: SegmentColors {
                                        normal: params.suffix_color,
                                        highlight: params.suffix_highlight_color,
                                    },
                                },
                            );
                            y += params.line_height;
                        } else if wrapped_suffix.is_empty() {
                            y += params.line_height;
                        } else {
                            unsafe {
                                SelectObject(hdc, params.small_font);
                                SetTextColor(hdc, params.suffix_color);
                            }
                            for row in wrapped_suffix {
                                draw_text_line(hdc, &row, params.scale.padding_x + bullet_width, y);
                                y += params.line_height;
                            }
                        }
                    }
                }
            }
            Line::Spacer => {
                y += params.line_height / 2;
            }
        }
    }
}
