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
        let recipe_palette = recipe_detail_palette(&state.settings.theme, &palette);
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
                            recipe_bg_color: recipe_palette.bg_color,
                            recipe_border_color: recipe_palette.border_color,
                            recipe_label_color: recipe_palette.label_color,
                            recipe_text_color: recipe_palette.text_color,
                            recipe_ingredient_highlight_color: recipe_palette
                                .ingredient_highlight_color,
                            recipe_selection_text_color: recipe_palette.selection_text_color,
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
                            recipe_bg_color: recipe_palette.bg_color,
                            recipe_border_color: recipe_palette.border_color,
                            recipe_label_color: recipe_palette.label_color,
                            recipe_text_color: recipe_palette.text_color,
                            recipe_ingredient_highlight_color: recipe_palette
                                .ingredient_highlight_color,
                            recipe_selection_text_color: recipe_palette.selection_text_color,
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
                            recipe_bg_color: recipe_palette.bg_color,
                            recipe_border_color: recipe_palette.border_color,
                            recipe_label_color: recipe_palette.label_color,
                            recipe_text_color: recipe_palette.text_color,
                            recipe_ingredient_highlight_color: recipe_palette
                                .ingredient_highlight_color,
                            recipe_selection_text_color: recipe_palette.selection_text_color,
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
                            recipe_bg_color: recipe_palette.bg_color,
                            recipe_border_color: recipe_palette.border_color,
                            recipe_label_color: recipe_palette.label_color,
                            recipe_text_color: recipe_palette.text_color,
                            recipe_ingredient_highlight_color: recipe_palette
                                .ingredient_highlight_color,
                            recipe_selection_text_color: recipe_palette.selection_text_color,
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
                    recipe_bg_color: recipe_palette.bg_color,
                    recipe_border_color: recipe_palette.border_color,
                    recipe_label_color: recipe_palette.label_color,
                    recipe_text_color: recipe_palette.text_color,
                    recipe_ingredient_highlight_color: recipe_palette.ingredient_highlight_color,
                    recipe_selection_text_color: recipe_palette.selection_text_color,
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
    recipe_bg_color: COLORREF,
    recipe_border_color: COLORREF,
    recipe_label_color: COLORREF,
    recipe_text_color: COLORREF,
    recipe_ingredient_highlight_color: COLORREF,
    recipe_selection_text_color: COLORREF,
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
                recipe_id,
                ingredient_alert,
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
                    draw_capture.layout.item_recipe_ids.push(*recipe_id);
                    draw_capture.layout.item_ingredient_flags.push(false);
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
                    if *ingredient_alert {
                        let alert_width = (styled_width - bullet_width)
                            .min(params.content_width - bullet_width)
                            .max(text_width(hdc, &clipped_main));
                        draw_outline_rect(
                            hdc,
                            &RECT {
                                left: line_x - 2,
                                top: y,
                                right: line_x + alert_width + 2,
                                bottom: y + params.line_height - 1,
                            },
                            params.recipe_border_color,
                        );
                    }
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
                        if *ingredient_alert {
                            draw_outline_rect(
                                hdc,
                                &RECT {
                                    left: line_x - 2,
                                    top: y,
                                    right: line_x + text_width(hdc, &row.text) + 2,
                                    bottom: y + params.line_height - 1,
                                },
                                params.recipe_border_color,
                            );
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
            Line::RecipeDetail { rows } => {
                y = draw_recipe_detail_block(
                    hdc,
                    rows,
                    y,
                    params.scale,
                    params.content_width,
                    params.line_height,
                    params.normal_font,
                    params.small_bold_font,
                    params.recipe_bg_color,
                    params.recipe_border_color,
                    params.recipe_label_color,
                    params.recipe_text_color,
                    params.recipe_ingredient_highlight_color,
                    params.recipe_selection_text_color,
                    bullet_width,
                    params.selection,
                    params.favorites,
                    capture.as_deref_mut(),
                );
            }
            Line::Spacer => {
                y += params.line_height / 2;
            }
        }
    }
}

fn draw_recipe_detail_block(
    hdc: HDC,
    rows: &[RecipeDetailRow],
    y: i32,
    scale: PopupScale,
    content_width: i32,
    line_height: i32,
    normal_font: HFONT,
    label_font: HFONT,
    bg_color: COLORREF,
    border_color: COLORREF,
    label_color: COLORREF,
    text_color: COLORREF,
    ingredient_highlight_color: COLORREF,
    selection_text_color: COLORREF,
    bullet_width: i32,
    selection: Option<&SelectionRange>,
    favorites: &FavoritesSnapshot,
    mut capture: Option<&mut DrawCapture>,
) -> i32 {
    let pad_x = scale_px(RECIPE_DETAIL_PAD_X, scale.factor);
    let pad_y = scale_px(RECIPE_DETAIL_PAD_Y, scale.factor);
    let row_gap = scale_px(RECIPE_DETAIL_ROW_GAP, scale.factor);
    let margin_y = scale_px(RECIPE_DETAIL_MARGIN_Y, scale.factor);
    let block_x = scale.padding_x + bullet_width;
    let block_width = (content_width - bullet_width).max(80);
    let inner_x = block_x + pad_x;
    let inner_width = (block_width - pad_x * 2).max(40);

    let layouts = recipe_detail_row_layouts(hdc, rows, normal_font, label_font, inner_width, pad_x);
    let content_rows = layouts
        .iter()
        .map(|layout| 1 + layout.value_lines.len())
        .sum::<usize>()
        .max(1);
    let gaps = row_gap * rows.len().saturating_sub(1) as i32;
    let block_top = y + margin_y;
    let block_height = pad_y * 2 + content_rows as i32 * line_height + gaps;
    let block_rect = RECT {
        left: block_x,
        top: block_top,
        right: block_x + block_width,
        bottom: block_top + block_height,
    };

    unsafe {
        let brush = CreateSolidBrush(bg_color);
        FillRect(hdc, &block_rect, brush);
        DeleteObject(brush);
    }
    draw_recipe_detail_border(hdc, &block_rect, border_color);

    let mut text_y = block_top + pad_y;
    for (idx, layout) in layouts.iter().enumerate() {
        let item_id = if layout.selectable {
            if let Some(ref mut draw_capture) = capture {
                let id = draw_capture.layout.items.len();
                draw_capture.layout.items.push(layout.value.clone());
                draw_capture.layout.item_recipe_ids.push(None);
                draw_capture.layout.item_ingredient_flags.push(true);
                Some(id)
            } else {
                None
            }
        } else {
            None
        };
        let selected_item_range =
            item_id.and_then(|id| selection.filter(|sel| sel.item_id == id).copied());

        unsafe {
            SelectObject(hdc, label_font);
            SetTextColor(hdc, label_color);
        }
        draw_text_line(hdc, &layout.label, inner_x, text_y);
        if let Some(first_value) = layout.inline_value.as_ref() {
            unsafe {
                SelectObject(hdc, normal_font);
                SetTextColor(hdc, text_color);
            }
            let value_x = inner_x + layout.label_width + scale_px(6, scale.factor);
            draw_recipe_value_row(
                hdc,
                first_value,
                LinePlacement {
                    x: value_x,
                    y: text_y,
                },
                RecipeValueStyle {
                    normal_font,
                    highlight_font: label_font,
                    normal_color: text_color,
                    highlight_color: ingredient_highlight_color,
                    selection_bg_color: ingredient_highlight_color,
                    selection_text_color,
                    line_height,
                },
                &favorites.ingredient_snippets_lower,
                selected_item_range,
            );
            if let Some(ref mut draw_capture) = capture {
                if let Some(item_id) = item_id {
                    add_selectable_row(
                        &mut draw_capture.layout,
                        item_id,
                        first_value,
                        RowCaptureContext {
                            bounds: RowBounds {
                                left: value_x,
                                top: text_y,
                                line_height,
                            },
                            hdc,
                            font: normal_font,
                        },
                    );
                }
            }
        }
        text_y += line_height;

        if !layout.value_lines.is_empty() {
            unsafe {
                SelectObject(hdc, normal_font);
                SetTextColor(hdc, text_color);
            }
            let value_x = inner_x + pad_x;
            for value_line in &layout.value_lines {
                draw_recipe_value_row(
                    hdc,
                    value_line,
                    LinePlacement {
                        x: value_x,
                        y: text_y,
                    },
                    RecipeValueStyle {
                        normal_font,
                        highlight_font: label_font,
                        normal_color: text_color,
                        highlight_color: ingredient_highlight_color,
                        selection_bg_color: ingredient_highlight_color,
                        selection_text_color,
                        line_height,
                    },
                    &favorites.ingredient_snippets_lower,
                    selected_item_range,
                );
                if let Some(ref mut draw_capture) = capture {
                    if let Some(item_id) = item_id {
                        add_selectable_row(
                            &mut draw_capture.layout,
                            item_id,
                            value_line,
                            RowCaptureContext {
                                bounds: RowBounds {
                                    left: value_x,
                                    top: text_y,
                                    line_height,
                                },
                                hdc,
                                font: normal_font,
                            },
                        );
                    }
                }
                text_y += line_height;
            }
        }
        if idx + 1 < layouts.len() {
            text_y += row_gap;
        }
    }

    block_rect.bottom + margin_y
}

#[derive(Debug, Clone, Copy)]
struct RecipeValueStyle {
    normal_font: HFONT,
    highlight_font: HFONT,
    normal_color: COLORREF,
    highlight_color: COLORREF,
    selection_bg_color: COLORREF,
    selection_text_color: COLORREF,
    line_height: i32,
}

fn draw_recipe_value_row(
    hdc: HDC,
    row: &WrappedRow,
    placement: LinePlacement,
    style: RecipeValueStyle,
    ingredient_snippets_lower: &[String],
    selection: Option<SelectionRange>,
) {
    let ranges = snippet_match_ranges(&row.text, ingredient_snippets_lower);
    let segments = segments_for_local_row(&row.text, &ranges);
    let mut cursor = placement.x;
    for (text, highlighted) in segments {
        let font = if highlighted {
            style.highlight_font
        } else {
            style.normal_font
        };
        unsafe {
            SelectObject(hdc, font);
            SetTextColor(
                hdc,
                if highlighted {
                    style.highlight_color
                } else {
                    style.normal_color
                },
            );
        }
        draw_text_line(hdc, &text, cursor, placement.y);
        cursor += text_width_with_font(hdc, font, &text);
    }

    if let Some(selection) = selection {
        draw_selection_bg_for_row(
            hdc,
            row,
            RowBounds {
                left: placement.x,
                top: placement.y,
                line_height: style.line_height,
            },
            SelectionOverlay {
                start: selection.start,
                end: selection.end,
                bg_color: style.selection_bg_color,
            },
        );
        draw_selected_row_text(
            hdc,
            row,
            placement,
            style.normal_font,
            style.selection_text_color,
            selection,
        );
    }
}

fn draw_selected_row_text(
    hdc: HDC,
    row: &WrappedRow,
    placement: LinePlacement,
    font: HFONT,
    color: COLORREF,
    selection: SelectionRange,
) {
    let start = max(row.start, selection.start);
    let end = min(row.end, selection.end);
    if start >= end {
        return;
    }
    let local_start = start.saturating_sub(row.start);
    let local_end = end.saturating_sub(row.start);
    let Some(prefix) = row.text.get(..local_start) else {
        return;
    };
    let Some(selected) = row.text.get(local_start..local_end) else {
        return;
    };
    let x = placement.x + text_width_with_font(hdc, font, prefix);
    unsafe {
        SelectObject(hdc, font);
        SetTextColor(hdc, color);
    }
    draw_text_line(hdc, selected, x, placement.y);
}

fn snippet_match_ranges(text: &str, snippets_lower: &[String]) -> Vec<(usize, usize)> {
    if text.is_empty() || snippets_lower.is_empty() {
        return Vec::new();
    }
    let lower_text = text.to_lowercase();
    let mut candidates = Vec::new();
    for snippet in snippets_lower {
        if snippet.is_empty() {
            continue;
        }
        let mut search_start = 0usize;
        while search_start < lower_text.len() {
            let Some(found) = lower_text[search_start..].find(snippet) else {
                break;
            };
            let start = search_start + found;
            let end = start + snippet.len();
            if text.get(start..end).is_some() {
                candidates.push((start, end));
            }
            search_start = end;
        }
    }
    candidates.sort_by(|a, b| {
        let len_a = a.1.saturating_sub(a.0);
        let len_b = b.1.saturating_sub(b.0);
        len_b.cmp(&len_a).then(a.0.cmp(&b.0))
    });

    let mut kept: Vec<(usize, usize)> = Vec::new();
    for range in candidates {
        if kept
            .iter()
            .any(|existing| max(existing.0, range.0) < min(existing.1, range.1))
        {
            continue;
        }
        kept.push(range);
    }
    kept.sort_by_key(|range| range.0);
    kept
}

fn segments_for_local_row(text: &str, ranges: &[(usize, usize)]) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for (start, end) in ranges {
        if cursor < *start {
            if let Some(normal) = text.get(cursor..*start) {
                out.push((normal.to_string(), false));
            }
        }
        if let Some(highlight) = text.get(*start..*end) {
            out.push((highlight.to_string(), true));
        }
        cursor = *end;
    }
    if cursor < text.len() {
        if let Some(rest) = text.get(cursor..text.len()) {
            out.push((rest.to_string(), false));
        }
    }
    if out.is_empty() {
        out.push((text.to_string(), false));
    }
    out
}

#[derive(Debug, Clone)]
struct RecipeDetailRowLayout {
    label: String,
    label_width: i32,
    value: String,
    selectable: bool,
    inline_value: Option<WrappedRow>,
    value_lines: Vec<WrappedRow>,
}

fn recipe_detail_row_layouts(
    hdc: HDC,
    rows: &[RecipeDetailRow],
    normal_font: HFONT,
    label_font: HFONT,
    inner_width: i32,
    value_indent: i32,
) -> Vec<RecipeDetailRowLayout> {
    let mut out = Vec::new();
    for row in rows {
        let label = format!("{}:", normalize_text(&row.label));
        let value = normalize_text(&row.value);
        let selectable = row.label.eq_ignore_ascii_case("Ingredients") && !value.is_empty();
        let label_width = text_width_with_font(hdc, label_font, &label);
        let value_width = text_width_with_font(hdc, normal_font, &value);
        let inline_width = label_width + 6 + value_width;
        if !value.is_empty() && inline_width <= inner_width {
            out.push(RecipeDetailRowLayout {
                label,
                label_width,
                value: value.clone(),
                selectable,
                inline_value: Some(WrappedRow {
                    text: value.clone(),
                    start: 0,
                    end: value.len(),
                }),
                value_lines: Vec::new(),
            });
        } else {
            let value_width = (inner_width - value_indent).max(32);
            out.push(RecipeDetailRowLayout {
                label,
                label_width,
                value: value.clone(),
                selectable,
                inline_value: None,
                value_lines: wrap_text_to_width_with_font_rows(
                    hdc,
                    normal_font,
                    &value,
                    value_width,
                ),
            });
        }
    }
    out
}

fn draw_recipe_detail_border(hdc: HDC, rect: &RECT, color: COLORREF) {
    draw_outline_rect(hdc, rect, color);
}

fn draw_outline_rect(hdc: HDC, rect: &RECT, color: COLORREF) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let top = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.top + 1,
        };
        let bottom = RECT {
            left: rect.left,
            top: rect.bottom - 1,
            right: rect.right,
            bottom: rect.bottom,
        };
        let left = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.left + 1,
            bottom: rect.bottom,
        };
        let right = RECT {
            left: rect.right - 1,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        FillRect(hdc, &top, brush);
        FillRect(hdc, &bottom, brush);
        FillRect(hdc, &left, brush);
        FillRect(hdc, &right, brush);
        DeleteObject(brush);
    }
}
