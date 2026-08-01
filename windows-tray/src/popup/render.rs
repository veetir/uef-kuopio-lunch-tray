//! Popup painting and text rendering helpers.

use super::animation::{current_animation_frame, hovered_header_button, pressed_header_button};
use super::content::{build_lines, current_favorites_snapshot};
use super::interaction::{
    clear_selection_layout, current_selection_range, recipe_detail_scroll_offset_px,
    store_selection_layout,
};
use super::layout::{
    create_fonts, flatten_suffix_segments, group_caption_bottom_gap, header_layout,
    header_marker_rects, header_title, popup_scale_for_dpi, scale_px, suffix_gap_width,
    text_metrics, text_width, text_width_with_font, text_with_suffix_width, wrap_text_to_width,
    wrap_text_to_width_with_font, wrap_text_to_width_with_font_rows, METADATA_BOTTOM_GAP_PX,
};
use super::theme::{lerp_color, recipe_detail_palette, rgb, theme_palette};
use super::*;

mod layer;
mod text;

pub(super) use layer::paint_popup;

#[cfg(feature = "bench")]
pub(in crate::popup) fn bench_favorite_match_range_count(
    value: &str,
    snippets_lower: &[String],
) -> usize {
    let favorites = FavoritesSnapshot {
        snippets_lower: snippets_lower.to_vec(),
        ingredient_snippets_lower: Vec::new(),
    };
    text::favorite_match_ranges(value, &favorites).len()
}
