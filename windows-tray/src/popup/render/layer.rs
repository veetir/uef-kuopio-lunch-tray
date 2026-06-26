//! High-level paint orchestration and line-by-line content rendering.

use super::text::{
    add_selectable_row, add_selectable_segmented_row, draw_header_button, draw_main_segments,
    draw_selection_bg_for_row, draw_selection_bg_for_segments, draw_text_line, draw_text_segments,
    favorite_match_ranges, fit_text_to_width, segments_for_row, text_segments_width, LinePlacement,
    RowBounds, RowCaptureContext, SegmentColors, SegmentFonts, SegmentStyle, SelectionOverlay,
};
use super::*;

const BREITKOPF_FRAKTUR_FONT: &[u8] = include_bytes!("../../../assets/fonts/BreitkopfFraktur.ttf");
const DIPLOMA_FONT: &[u8] = include_bytes!("../../../assets/fonts/diploma.ttf");
static BREITKOPF_FRAKTUR_LOADED: OnceLock<bool> = OnceLock::new();
static DIPLOMA_LOADED: OnceLock<bool> = OnceLock::new();

struct HighlightFontDef {
    bytes: &'static [u8],
    loaded: &'static OnceLock<bool>,
    face: &'static str,
    point_size: i32,
}

fn highlight_font_def(
    highlight_theme: crate::settings::HighlightTheme,
) -> Option<HighlightFontDef> {
    match highlight_theme {
        crate::settings::HighlightTheme::Fraktur => Some(HighlightFontDef {
            bytes: BREITKOPF_FRAKTUR_FONT,
            loaded: &BREITKOPF_FRAKTUR_LOADED,
            face: "Breitkopf Fraktur",
            point_size: 16,
        }),
        crate::settings::HighlightTheme::Diploma => Some(HighlightFontDef {
            bytes: DIPLOMA_FONT,
            loaded: &DIPLOMA_LOADED,
            face: "Diploma",
            point_size: 16,
        }),
        crate::settings::HighlightTheme::Default => None,
    }
}

fn ensure_highlight_font_loaded(def: &HighlightFontDef) -> bool {
    *def.loaded.get_or_init(|| unsafe {
        let mut font_count = 0u32;
        let handle = AddFontMemResourceEx(
            def.bytes.as_ptr() as *const c_void,
            def.bytes.len() as u32,
            None,
            &mut font_count,
        );
        handle.0 != 0 && font_count > 0
    })
}

fn create_highlight_font(
    hdc: HDC,
    scale_factor: f32,
    fallback_font: HFONT,
    highlight_theme: crate::settings::HighlightTheme,
) -> HFONT {
    let Some(def) = highlight_font_def(highlight_theme) else {
        return fallback_font;
    };
    if !ensure_highlight_font_loaded(&def) {
        return fallback_font;
    }
    unsafe {
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        let height = -MulDiv(scale_px(def.point_size, scale_factor).max(9), dpi_y, 72);
        let face = to_wstring(def.face);
        let font = CreateFontW(
            height,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            PCWSTR(face.as_ptr()),
        );
        if font.0 == 0 {
            fallback_font
        } else {
            font
        }
    }
}

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
        let highlight_font =
            create_highlight_font(hdc, scale.factor, bold_font, state.settings.highlight_theme);
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
                            highlight_font,
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
                            highlight_font,
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
                            highlight_font,
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
                            highlight_font,
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
                    highlight_font,
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
        if highlight_font.0 != bold_font.0 {
            DeleteObject(highlight_font);
        }
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
    highlight_font: HFONT,
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
    for (line_index, line) in lines.iter().enumerate() {
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
            Line::Subheading {
                text,
                reserve_prefix,
            } => {
                unsafe {
                    SelectObject(hdc, params.small_font);
                    SetTextColor(hdc, params.suffix_color);
                }
                let prefix_width = shared_prefix_width_for_prefix(
                    hdc,
                    lines,
                    params.normal_font,
                    reserve_prefix.as_deref(),
                );
                let indent = params.scale.padding_x + bullet_width + prefix_width;
                let wrapped = wrap_text_to_width_with_font(
                    hdc,
                    params.small_font,
                    text,
                    (params.content_width - bullet_width - prefix_width).max(24),
                );
                if wrapped.is_empty() {
                    y += params.line_height;
                } else {
                    for row in wrapped {
                        draw_text_line(hdc, &row, indent, y + 1);
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
                show_bullet,
                price_prefix,
                reserve_prefix,
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
                let prefix = price_prefix.as_deref().or(reserve_prefix.as_deref());
                let prefix_width =
                    shared_prefix_width_for_prefix(hdc, lines, params.normal_font, prefix);
                let styled_width = text_with_suffix_width(
                    hdc,
                    params.normal_font,
                    params.small_font,
                    params.small_bold_font,
                    main,
                    suffix_segments,
                    bullet_width + prefix_width,
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
                    let aligned_main_width = inline_suffix_alignment_width(
                        hdc,
                        lines,
                        line_index,
                        bullet_width,
                        InlineSuffixAlignmentParams {
                            normal_font: params.normal_font,
                            highlight_font: params.highlight_font,
                            small_font: params.small_font,
                            small_bold_font: params.small_bold_font,
                            content_width: params.content_width,
                            favorites: params.favorites,
                        },
                    );
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
                    let max_main =
                        (params.content_width - bullet_width - prefix_width - suffix_width - 4)
                            .max(24);
                    unsafe {
                        SelectObject(hdc, params.normal_font);
                        SetTextColor(hdc, params.body_text_color);
                    }
                    let clipped_main = fit_text_to_width(hdc, main, max_main);
                    let line_x = params.scale.padding_x + bullet_width;
                    let main_x = line_x + prefix_width;
                    let row = WrappedRow {
                        start: 0,
                        end: clipped_main.len(),
                        text: clipped_main.clone(),
                    };
                    let row_segments =
                        segments_for_row(&clipped_main, row.start, row.end, &favorite_ranges);
                    let main_segment_fonts = SegmentFonts {
                        normal: params.normal_font,
                        highlight: params.highlight_font,
                    };
                    let main_width = text_segments_width(
                        hdc,
                        &row_segments,
                        params.normal_font,
                        params.highlight_font,
                    );
                    if *show_bullet {
                        draw_text_line(hdc, BULLET_PREFIX, params.scale.padding_x, y);
                    }
                    if let Some(prefix) = price_prefix.as_deref() {
                        draw_text_line(hdc, prefix, line_x, y);
                    }
                    if *ingredient_alert {
                        draw_outline_rect(
                            hdc,
                            &RECT {
                                left: main_x - 2,
                                top: y,
                                right: main_x + main_width + 2,
                                bottom: y + params.line_height - 1,
                            },
                            params.recipe_border_color,
                        );
                    }
                    if let Some(selection) = selected_item_range {
                        draw_selection_bg_for_segments(
                            hdc,
                            &row,
                            &row_segments,
                            RowBounds {
                                left: main_x,
                                top: y,
                                line_height: params.line_height,
                            },
                            SelectionOverlay {
                                start: selection.start,
                                end: selection.end,
                                bg_color: params.selection_bg_color,
                            },
                            main_segment_fonts,
                        );
                    }
                    draw_main_segments(
                        hdc,
                        &row_segments,
                        LinePlacement { x: main_x, y },
                        SegmentStyle {
                            fonts: main_segment_fonts,
                            colors: SegmentColors {
                                normal: params.body_text_color,
                                highlight: params.favorite_highlight_color,
                            },
                        },
                    );
                    if let Some(ref mut draw_capture) = capture {
                        add_selectable_segmented_row(
                            &mut draw_capture.layout,
                            item_id.unwrap_or(0),
                            &row,
                            RowCaptureContext {
                                bounds: RowBounds {
                                    left: main_x,
                                    top: y,
                                    line_height: params.line_height,
                                },
                                hdc,
                                font: params.normal_font,
                            },
                            &row_segments,
                            main_segment_fonts,
                        );
                    }
                    if !suffix_segments.is_empty() {
                        let tight_suffix_x = main_x + main_width + 4;
                        let aligned_suffix_x = aligned_main_width.map(|width| main_x + width + 4);
                        let right_edge = params.scale.padding_x + params.content_width;
                        let suffix_x = aligned_suffix_x
                            .filter(|x| *x + suffix_width <= right_edge)
                            .unwrap_or(tight_suffix_x);
                        if suffix_x + suffix_width <= right_edge {
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
                                        highlight: params.small_bold_font,
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
                    (main_wrap_width - prefix_width).max(24),
                );
                if wrapped_main.is_empty() {
                    y += params.line_height;
                } else {
                    for (idx, row) in wrapped_main.iter().enumerate() {
                        let line_x = params.scale.padding_x + bullet_width;
                        let main_x = line_x + prefix_width;
                        if idx == 0 {
                            if *show_bullet {
                                draw_text_line(hdc, BULLET_PREFIX, params.scale.padding_x, y);
                            }
                            if let Some(prefix) = price_prefix.as_deref() {
                                draw_text_line(hdc, prefix, line_x, y);
                            }
                        }
                        if *ingredient_alert {
                            draw_outline_rect(
                                hdc,
                                &RECT {
                                    left: main_x - 2,
                                    top: y,
                                    right: main_x + text_width(hdc, &row.text) + 2,
                                    bottom: y + params.line_height - 1,
                                },
                                params.recipe_border_color,
                            );
                        }
                        let row_segments =
                            segments_for_row(main, row.start, row.end, &favorite_ranges);
                        let main_segment_fonts = SegmentFonts {
                            normal: params.normal_font,
                            highlight: params.highlight_font,
                        };
                        if let Some(selection) = selected_item_range {
                            draw_selection_bg_for_segments(
                                hdc,
                                row,
                                &row_segments,
                                RowBounds {
                                    left: main_x,
                                    top: y,
                                    line_height: params.line_height,
                                },
                                SelectionOverlay {
                                    start: selection.start,
                                    end: selection.end,
                                    bg_color: params.selection_bg_color,
                                },
                                main_segment_fonts,
                            );
                        }
                        draw_main_segments(
                            hdc,
                            &row_segments,
                            LinePlacement { x: main_x, y },
                            SegmentStyle {
                                fonts: main_segment_fonts,
                                colors: SegmentColors {
                                    normal: params.body_text_color,
                                    highlight: params.favorite_highlight_color,
                                },
                            },
                        );
                        if let Some(ref mut draw_capture) = capture {
                            add_selectable_segmented_row(
                                &mut draw_capture.layout,
                                item_id.unwrap_or(0),
                                row,
                                RowCaptureContext {
                                    bounds: RowBounds {
                                        left: main_x,
                                        top: y,
                                        line_height: params.line_height,
                                    },
                                    hdc,
                                    font: params.normal_font,
                                },
                                &row_segments,
                                main_segment_fonts,
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
                            (params.content_width - bullet_width - prefix_width).max(24),
                        );
                        if wrapped_suffix.len() == 1 {
                            draw_text_segments(
                                hdc,
                                suffix_segments,
                                LinePlacement {
                                    x: params.scale.padding_x + bullet_width + prefix_width,
                                    y: y + 1,
                                },
                                SegmentStyle {
                                    fonts: SegmentFonts {
                                        normal: params.small_font,
                                        highlight: params.small_bold_font,
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
                                draw_text_line(
                                    hdc,
                                    &row,
                                    params.scale.padding_x + bullet_width + prefix_width,
                                    y,
                                );
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

fn inline_suffix_alignment_width(
    hdc: HDC,
    lines: &[Line],
    line_index: usize,
    bullet_width: i32,
    params: InlineSuffixAlignmentParams<'_>,
) -> Option<i32> {
    let Line::MenuItem { .. } = lines.get(line_index)? else {
        return None;
    };

    let start = lines[..line_index]
        .iter()
        .rposition(|line| !matches!(line, Line::MenuItem { .. }))
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let end = lines[line_index..]
        .iter()
        .position(|line| !matches!(line, Line::MenuItem { .. }))
        .map(|idx| line_index + idx)
        .unwrap_or(lines.len());

    let mut max_width: Option<i32> = None;
    let mut candidates = 0usize;
    for line in &lines[start..end] {
        let Line::MenuItem {
            price_prefix,
            reserve_prefix,
            main,
            suffix_segments,
            ..
        } = line
        else {
            continue;
        };
        if suffix_segments.is_empty() {
            continue;
        }
        let prefix_width = price_prefix
            .as_deref()
            .or(reserve_prefix.as_deref())
            .map(|prefix| text_width_with_font(hdc, params.normal_font, prefix))
            .unwrap_or(0);
        let styled_width = text_with_suffix_width(
            hdc,
            params.normal_font,
            params.small_font,
            params.small_bold_font,
            main,
            suffix_segments,
            bullet_width + prefix_width,
        );
        if styled_width > params.content_width {
            continue;
        }

        let favorite_ranges = favorite_match_ranges(main, params.favorites);
        let row_segments = segments_for_row(main, 0, main.len(), &favorite_ranges);
        let width = text_segments_width(
            hdc,
            &row_segments,
            params.normal_font,
            params.highlight_font,
        );
        max_width = Some(max_width.map_or(width, |current| current.max(width)));
        candidates += 1;
    }

    if candidates >= 2 {
        max_width
    } else {
        None
    }
}

fn shared_prefix_width_for_prefix(
    hdc: HDC,
    lines: &[Line],
    normal_font: HFONT,
    prefix: Option<&str>,
) -> i32 {
    let Some(prefix) = prefix else {
        return 0;
    };
    let bucket = price_prefix_bucket(prefix);
    if bucket == 0 {
        return text_width_with_font(hdc, normal_font, prefix);
    }
    lines
        .iter()
        .filter_map(|line| prefix_for_line(line))
        .filter(|candidate| price_prefix_bucket(candidate) == bucket)
        .map(|candidate| text_width_with_font(hdc, normal_font, candidate))
        .max()
        .unwrap_or_else(|| text_width_with_font(hdc, normal_font, prefix))
}

fn prefix_for_line(line: &Line) -> Option<&str> {
    match line {
        Line::Subheading { reserve_prefix, .. } => reserve_prefix.as_deref(),
        Line::MenuItem {
            price_prefix,
            reserve_prefix,
            ..
        } => price_prefix.as_deref().or(reserve_prefix.as_deref()),
        _ => None,
    }
}

fn price_prefix_bucket(prefix: &str) -> usize {
    prefix.chars().filter(|ch| *ch == '€').count()
}

#[derive(Debug, Clone, Copy)]
struct InlineSuffixAlignmentParams<'a> {
    normal_font: HFONT,
    highlight_font: HFONT,
    small_font: HFONT,
    small_bold_font: HFONT,
    content_width: i32,
    favorites: &'a FavoritesSnapshot,
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
    let content_height = content_rows as i32 * line_height + gaps;
    let viewport_rows = content_rows.min(RECIPE_DETAIL_MAX_VISIBLE_ROWS).max(1);
    let viewport_height = if content_rows <= RECIPE_DETAIL_MAX_VISIBLE_ROWS {
        content_height
    } else {
        viewport_rows as i32 * line_height
    };
    let max_scroll_offset = (content_height - viewport_height).max(0);
    let scroll_offset = recipe_detail_scroll_offset_px().min(max_scroll_offset);
    let block_height = pad_y * 2 + viewport_height;
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
    if let Some(ref mut draw_capture) = capture {
        draw_capture.layout.recipe_scroll_rect = Some(block_rect);
        draw_capture.layout.recipe_scroll_max_offset_px = max_scroll_offset;
        draw_capture.layout.recipe_scroll_line_height = line_height;
    }
    if max_scroll_offset > 0 {
        draw_recipe_detail_scrollbar(
            hdc,
            &block_rect,
            pad_y,
            scale_px(RECIPE_DETAIL_SCROLLBAR_WIDTH, scale.factor).max(3),
            scroll_offset,
            max_scroll_offset,
            viewport_height,
            content_height,
            border_color,
            label_color,
        );
    }

    let clip_rect = RECT {
        left: block_rect.left + 1,
        top: block_top + pad_y,
        right: block_rect.right
            - if max_scroll_offset > 0 {
                scale_px(RECIPE_DETAIL_SCROLLBAR_WIDTH, scale.factor).max(3) + 2
            } else {
                1
            },
        bottom: block_top + pad_y + viewport_height,
    };
    let saved_dc = unsafe { SaveDC(hdc) };
    unsafe {
        IntersectClipRect(
            hdc,
            clip_rect.left,
            clip_rect.top,
            clip_rect.right,
            clip_rect.bottom,
        );
    }

    let mut text_y = block_top + pad_y - scroll_offset;
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
                    highlight_font: normal_font,
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
                    if row_intersects_clip(text_y, line_height, &clip_rect) {
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
                        highlight_font: normal_font,
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
                        if row_intersects_clip(text_y, line_height, &clip_rect) {
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
                }
                text_y += line_height;
            }
        }
        if idx + 1 < layouts.len() {
            text_y += row_gap;
        }
    }
    unsafe {
        RestoreDC(hdc, saved_dc);
    }

    block_rect.bottom + margin_y
}

fn row_intersects_clip(y: i32, line_height: i32, clip_rect: &RECT) -> bool {
    y + line_height > clip_rect.top && y < clip_rect.bottom
}

fn draw_recipe_detail_scrollbar(
    hdc: HDC,
    block_rect: &RECT,
    pad_y: i32,
    width: i32,
    offset: i32,
    max_offset: i32,
    viewport_height: i32,
    content_height: i32,
    track_color: COLORREF,
    thumb_color: COLORREF,
) {
    if max_offset <= 0 || content_height <= 0 || viewport_height <= 0 {
        return;
    }
    let track_top = block_rect.top + pad_y;
    let track_bottom = block_rect.bottom - pad_y;
    let track_height = (track_bottom - track_top).max(1);
    let thumb_height = ((track_height * viewport_height) / content_height).clamp(12, track_height);
    let travel = (track_height - thumb_height).max(0);
    let thumb_top = track_top + ((travel * offset) / max_offset.max(1));
    let track_rect = RECT {
        left: block_rect.right - width - 3,
        top: track_top,
        right: block_rect.right - 3,
        bottom: track_bottom,
    };
    let thumb_rect = RECT {
        left: track_rect.left,
        top: thumb_top,
        right: track_rect.right,
        bottom: thumb_top + thumb_height,
    };
    unsafe {
        let track_brush = CreateSolidBrush(track_color);
        FillRect(hdc, &track_rect, track_brush);
        DeleteObject(track_brush);
        let thumb_brush = CreateSolidBrush(thumb_color);
        FillRect(hdc, &thumb_rect, thumb_brush);
        DeleteObject(thumb_brush);
    }
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
        let selectable = row.selectable && !value.is_empty();
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
