//! Chrome edge vocabulary.
//!
//! A theme picks one border style and every piece of chrome — the popup frame,
//! the header buttons, the recipe panel — draws its edges the same way. Edges
//! read as a system: a raised frame above flat buttons looks like a mistake
//! rather than a choice, so this is deliberately one knob and not four.
//!
//! The bevel styles reproduce the classic Windows four-colour edge, but the
//! colours are derived from whatever face colour the caller passes rather than
//! hardcoded greys, so a theme in any hue can use them.

use super::theme::{lerp_color, rgb};
use super::*;

/// How a theme draws the edges of its chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BorderStyle {
    /// No popup frame. Panels still get their own flat outline.
    None,
    /// 1px solid outline in the theme's border colour.
    Flat,
    /// Two-pixel bevel that reads as standing above the surface.
    Raised,
    /// Two-pixel bevel that reads as cut into the surface.
    Sunken,
}

impl BorderStyle {
    pub(super) fn from_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "flat" => Some(Self::Flat),
            "raised" => Some(Self::Raised),
            "sunken" => Some(Self::Sunken),
            _ => None,
        }
    }

    /// The style a panel or well uses under this vocabulary.
    ///
    /// Panels are content rather than chrome, so they keep a visible edge even
    /// when the theme asks for no popup frame. Bevel themes sink their panels,
    /// which is what the classic idiom does with list and text wells.
    pub(super) fn panel_style(self) -> Self {
        match self {
            Self::Raised | Self::Sunken => Self::Sunken,
            _ => Self::Flat,
        }
    }

    /// The style a button uses at rest, and pressed.
    pub(super) fn button_style(self, pressed: bool) -> Self {
        match self {
            Self::Raised | Self::Sunken => {
                if pressed {
                    Self::Sunken
                } else {
                    Self::Raised
                }
            }
            other => other,
        }
    }

    /// Whether a pressed button should shift its fill.
    ///
    /// Bevel themes signal the press by flipping the edge, the way a real
    /// push button does, so shifting the fill on top of that reads as muddy.
    pub(super) fn press_shifts_fill(self) -> bool {
        !matches!(self, Self::Raised | Self::Sunken)
    }
}

/// Built-in theme defaults. Custom themes override this via `themes.json`.
///
/// Only Grandpa ships with a frame. A 1px flat perimeter reads as sharp and
/// cheap at every scale we tried it, and it doubles up with the header divider
/// at the top edge; separation from the desktop is the drop shadow's job. The
/// flat style stays in the vocabulary for custom themes and for panels, which
/// need an outline regardless of what the frame does.
pub(super) fn border_style_for_theme(theme: &str) -> BorderStyle {
    match theme {
        "grandpa" | "grandma" => BorderStyle::Raised,
        "light" | "dark" | "blue" | "teletext1" | "teletext2" | "amber" | "green" => {
            BorderStyle::None
        }
        _ => crate::custom_themes::find_custom_theme(theme)
            .and_then(|custom| BorderStyle::from_name(custom.border.name()))
            .unwrap_or(BorderStyle::None),
    }
}

/// Whether the popup casts a drop shadow.
///
/// On for everything except Grandpa: Windows 95 drew no shadow under windows or
/// menus, so one there would be the single most anachronistic thing on screen.
pub(super) fn theme_shadow_enabled(theme: &str) -> bool {
    match theme {
        "grandpa" | "grandma" => false,
        _ => crate::custom_themes::find_custom_theme(theme)
            .map(|custom| custom.shadow)
            .unwrap_or(true),
    }
}

/// A chrome edge: the vocabulary to draw in, plus the colour the flat style
/// uses. The two always travel together, so callers pass them as one value.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChromeEdge {
    pub(super) style: BorderStyle,
    pub(super) color: COLORREF,
}

impl ChromeEdge {
    pub(super) fn panel(self) -> Self {
        Self {
            style: self.style.panel_style(),
            ..self
        }
    }

    pub(super) fn button(self, pressed: bool) -> Self {
        Self {
            style: self.style.button_style(pressed),
            ..self
        }
    }
}

/// The four tones of a classic 3D edge, derived from a face colour.
///
/// The two mid-tones are mixed from the face so they carry its hue, while the
/// extremes stay pure white and pure black exactly as the original palette did
/// (`BTNHIGHLIGHT` and `3DDKSHADOW` were never tinted). Against the canonical
/// 192,192,192 face this reproduces the originals: 223, 255, 128, 0.
#[derive(Debug, Clone, Copy)]
struct EdgeColors {
    light: COLORREF,
    highlight: COLORREF,
    shadow: COLORREF,
    dark: COLORREF,
}

fn edge_colors(face: COLORREF) -> EdgeColors {
    let white = rgb(255, 255, 255);
    let black = rgb(0, 0, 0);
    EdgeColors {
        light: lerp_color(face, white, 0.5),
        highlight: white,
        shadow: lerp_color(face, black, 0.33),
        dark: black,
    }
}

/// Draws the edge of `rect` in the given style.
///
/// `face` is the surface the edge sits on and drives the bevel tones.
pub(super) fn draw_edge(hdc: HDC, rect: &RECT, edge: ChromeEdge, face: COLORREF) {
    if rect.right - rect.left < 2 || rect.bottom - rect.top < 2 {
        return;
    }
    match edge.style {
        BorderStyle::None => {}
        BorderStyle::Flat => draw_ring(hdc, rect, 0, edge.color, edge.color),
        BorderStyle::Raised => {
            let colors = edge_colors(face);
            draw_ring(hdc, rect, 0, colors.light, colors.dark);
            draw_ring(hdc, rect, 1, colors.highlight, colors.shadow);
        }
        BorderStyle::Sunken => {
            let colors = edge_colors(face);
            draw_ring(hdc, rect, 0, colors.shadow, colors.highlight);
            draw_ring(hdc, rect, 1, colors.dark, colors.light);
        }
    }
}

/// One 1px ring inset from `rect`, top/left in `tl` and bottom/right in `br`.
///
/// The top and left runs stop a pixel short while the bottom and right runs go
/// full width, which is what gives a bevel its mitred corners instead of a
/// square outline in two colours.
fn draw_ring(hdc: HDC, rect: &RECT, inset: i32, tl: COLORREF, br: COLORREF) {
    let left = rect.left + inset;
    let top = rect.top + inset;
    let right = rect.right - inset;
    let bottom = rect.bottom - inset;
    if right - left < 1 || bottom - top < 1 {
        return;
    }
    fill(hdc, left, top, right - 1, top + 1, tl);
    fill(hdc, left, top, left + 1, bottom - 1, tl);
    fill(hdc, left, bottom - 1, right, bottom, br);
    fill(hdc, right - 1, top, right, bottom, br);
}

fn fill(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32, color: COLORREF) {
    if right <= left || bottom <= top {
        return;
    }
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(
            hdc,
            &RECT {
                left,
                top,
                right,
                bottom,
            },
            brush,
        );
        DeleteObject(brush);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(color: COLORREF) -> (u8, u8, u8) {
        let value = color.0;
        (
            (value & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            ((value >> 16) & 0xFF) as u8,
        )
    }

    #[test]
    fn border_style_names_decode() {
        assert_eq!(BorderStyle::from_name("none"), Some(BorderStyle::None));
        assert_eq!(BorderStyle::from_name("Flat"), Some(BorderStyle::Flat));
        assert_eq!(
            BorderStyle::from_name(" raised "),
            Some(BorderStyle::Raised)
        );
        assert_eq!(BorderStyle::from_name("sunken"), Some(BorderStyle::Sunken));
        assert_eq!(BorderStyle::from_name("groovy"), None);
    }

    #[test]
    fn bevel_tones_track_the_classic_grey_face() {
        let colors = edge_colors(rgb(192, 192, 192));

        assert_eq!(channels(colors.light), (223, 223, 223));
        assert_eq!(channels(colors.highlight), (255, 255, 255));
        assert_eq!(channels(colors.shadow), (128, 128, 128));
        assert_eq!(channels(colors.dark), (0, 0, 0));
    }

    #[test]
    fn bevel_tones_follow_the_face_hue() {
        let colors = edge_colors(rgb(180, 60, 120));
        let (lr, lg, lb) = channels(colors.light);
        let (sr, sg, sb) = channels(colors.shadow);

        assert!(lr > lg && lb > lg, "light mid-tone keeps the face hue");
        assert!(sr > sg && sb > sg, "shadow mid-tone keeps the face hue");
    }

    #[test]
    fn panels_stay_outlined_even_without_a_popup_frame() {
        assert_eq!(BorderStyle::None.panel_style(), BorderStyle::Flat);
        assert_eq!(BorderStyle::Flat.panel_style(), BorderStyle::Flat);
        assert_eq!(BorderStyle::Raised.panel_style(), BorderStyle::Sunken);
    }

    #[test]
    fn bevel_buttons_flip_their_edge_instead_of_darkening() {
        assert_eq!(
            BorderStyle::Raised.button_style(true),
            BorderStyle::Sunken,
            "pressed bevel button sinks"
        );
        assert_eq!(BorderStyle::Raised.button_style(false), BorderStyle::Raised);
        assert!(!BorderStyle::Raised.press_shifts_fill());
        assert!(BorderStyle::Flat.press_shifts_fill());
    }

    #[test]
    fn only_the_bevel_themes_have_a_frame() {
        assert_eq!(border_style_for_theme("grandpa"), BorderStyle::Raised);
        assert_eq!(border_style_for_theme("grandma"), BorderStyle::Raised);
        for theme in [
            "light",
            "dark",
            "blue",
            "teletext1",
            "teletext2",
            "amber",
            "green",
        ] {
            assert_eq!(
                border_style_for_theme(theme),
                BorderStyle::None,
                "{theme} should be frameless"
            );
        }
    }

    #[test]
    fn frameless_themes_still_outline_their_panels() {
        assert_eq!(
            border_style_for_theme("teletext1").panel_style(),
            BorderStyle::Flat
        );
    }

    #[test]
    fn only_the_bevel_themes_skip_the_shadow() {
        assert!(!theme_shadow_enabled("grandpa"));
        assert!(!theme_shadow_enabled("grandma"));
        for theme in ["light", "dark", "blue", "teletext1", "amber", "green"] {
            assert!(theme_shadow_enabled(theme), "{theme} should cast a shadow");
        }
    }
}
