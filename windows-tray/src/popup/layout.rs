//! Popup sizing, wrapping, cache, and placement helpers.

use super::animation::{begin_open_animation, clear_animation_state, clear_header_button_press};
use super::content::build_lines;
use super::interaction::clear_selection_state;
use super::theme::theme_font_family;
use super::*;

mod cache;
mod text;
mod window;

pub(super) use cache::invalidate_layout_budget_cache;
pub(super) use text::{
    flatten_suffix_segments, text_metrics, text_width, text_width_with_font,
    text_with_suffix_width, wrap_text_to_width, wrap_text_to_width_with_font,
    wrap_text_to_width_with_font_rows,
};
pub(super) use window::{
    create_fonts, header_layout, header_title, hide_popup, resize_popup_keep_position, show_popup,
    show_popup_at, show_popup_for_tray_icon,
};

pub(super) fn scale_px(base: i32, factor: f32) -> i32 {
    ((base as f32) * factor).round() as i32
}

fn widget_scale_factor(value: &str) -> f32 {
    match value {
        "125" => 1.25,
        "150" => 1.50,
        _ => 1.0,
    }
}

pub(super) fn popup_scale(settings: &Settings) -> PopupScale {
    let factor = widget_scale_factor(&settings.widget_scale);
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
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LineLayoutMetrics {
    required_content_width: i32,
    wrapped_line_count: usize,
}
