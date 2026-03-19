//! Popup painting and text rendering helpers.

use super::animation::{current_animation_frame, pressed_header_button};
use super::content::{build_lines, current_favorites_snapshot};
use super::interaction::{clear_selection_layout, current_selection_range, store_selection_layout};
use super::layout::{
    create_fonts, flatten_suffix_segments, header_layout, header_title, popup_scale, scale_px,
    text_metrics, text_width, text_width_with_font, text_with_suffix_width, wrap_text_to_width,
    wrap_text_to_width_with_font, wrap_text_to_width_with_font_rows,
};
use super::theme::{lerp_color, rgb, theme_palette};
use super::*;

mod layer;
mod text;

pub(super) use layer::paint_popup;
