//! Popup sizing, wrapping, cache, and placement helpers.

use super::animation::{begin_open_animation, clear_animation_state, clear_header_button_press};
use super::content::build_lines;
use super::interaction::clear_selection_state;
use super::theme::theme_font_family;
use super::*;

mod cache;
mod text;
mod window;

pub(super) const METADATA_BOTTOM_GAP_PX: i32 = 4;
pub(super) const GROUP_CAPTION_BOTTOM_GAP_PX: i32 = 6;

/// Extra space below a group caption in the standard/compact layouts.
///
/// The caption names the items *above* it, so without a break after it the
/// caption sits equidistant from its own items and the next group's price line
/// and grouping becomes ambiguous. Skipped on a trailing caption so the popup
/// does not grow a few pixels of dead space at the bottom.
///
/// Measurement and rendering both call this so their heights cannot drift.
pub(super) fn group_caption_bottom_gap(lines: &[Line], index: usize) -> i32 {
    if index + 1 < lines.len() {
        GROUP_CAPTION_BOTTOM_GAP_PX
    } else {
        0
    }
}

pub(super) use cache::invalidate_layout_budget_cache;
pub(super) use text::{
    flatten_suffix_segments, suffix_gap_width, text_metrics, text_width, text_width_with_font,
    text_with_suffix_width, wrap_text_to_width, wrap_text_to_width_with_font,
    wrap_text_to_width_with_font_rows,
};
pub(super) use window::{
    create_fonts, header_layout, header_marker_rects, header_title, hide_popup,
    resize_popup_keep_position, show_popup, show_popup_at, show_popup_for_tray_icon,
};

pub(super) fn scale_px(base: i32, factor: f32) -> i32 {
    ((base as f32) * factor).round() as i32
}

fn widget_scale_factor(value: &str) -> f32 {
    match value {
        "small" => 1.0,
        "large" => 1.50,
        _ => 1.25,
    }
}

pub(super) fn popup_scale_for_dpi(settings: &Settings, dpi_y: i32) -> PopupScale {
    let dpi_factor = (dpi_y.max(1) as f32) / (BASE_DPI as f32);
    let factor = widget_scale_factor(&settings.widget_scale) * dpi_factor;
    let padding_x = scale_px(PADDING_X, factor).max(8);
    let padding_y = scale_px(PADDING_Y, factor).max(6);
    let min_width = scale_px(POPUP_MIN_WIDTH, factor).max(220);
    let max_width = scale_px(POPUP_MAX_WIDTH, factor).max(min_width);
    let max_content_width = (max_width - padding_x * 2).max(40);
    let min_content_width = (min_width - padding_x * 2).max(40);

    PopupScale {
        factor,
        padding_x,
        padding_y,
        line_gap: scale_px(LINE_GAP, factor).max(1),
        anchor_gap: scale_px(ANCHOR_GAP, factor).max(0),
        max_width,
        min_width,
        max_content_width,
        min_content_width,
        header_height: scale_px(HEADER_HEIGHT, factor).max(30),
        header_button_size: scale_px(HEADER_BUTTON_SIZE, factor).max(18),
        header_button_gap: scale_px(HEADER_BUTTON_GAP, factor).max(4),
        switch_offset_px: scale_px(POPUP_SWITCH_OFFSET_PX, factor).max(2),
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CachedLayoutBudget {
    max_wrapped_lines: Option<usize>,
    max_content_width_px: Option<i32>,
    max_extra_height_px: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caption(text: &str) -> Line {
        Line::Subheading {
            text: text.to_string(),
            reserve_prefix: None,
        }
    }

    #[test]
    fn group_caption_gap_separates_following_groups() {
        let lines = vec![caption("Salad buffet"), Line::Text("next group".to_string())];

        assert_eq!(
            group_caption_bottom_gap(&lines, 0),
            GROUP_CAPTION_BOTTOM_GAP_PX
        );
    }

    #[test]
    fn group_caption_gap_omitted_on_trailing_caption() {
        let lines = vec![Line::Text("item".to_string()), caption("Dessert")];

        assert_eq!(group_caption_bottom_gap(&lines, 1), 0);
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LineLayoutMetrics {
    required_content_width: i32,
    wrapped_line_count: usize,
    extra_height_px: i32,
}
