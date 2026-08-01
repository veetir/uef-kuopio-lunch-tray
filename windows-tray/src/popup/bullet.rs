//! Menu item bullet marks.
//!
//! Bullets are drawn geometrically rather than typed as a font glyph. A glyph's
//! size, weight, and vertical centring come from whichever family the theme
//! picked, so the same bullet rendered noticeably differently across Segoe UI,
//! Tahoma, and Consolas. Drawing the mark ourselves keeps it consistent, lets it
//! sit on the text's optical centre by construction, and makes the shape a theme
//! property.

use super::layout::text_metrics;
use super::theme::{lerp_color, rgb, ThemePalette};
use super::*;

/// Shape of the mark drawn beside a menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BulletStyle {
    /// Right-pointing triangle. Closest to the original `▸` glyph.
    Triangle,
    /// Solid square. Suits the block-graphics themes.
    Square,
    /// Solid diamond. Reads more mechanical; suits the terminal themes.
    Diamond,
    /// Raised 1px chip in the Windows 95 idiom: light top/left, dark
    /// bottom/right, theme button face between.
    Bevel,
    /// No mark at all, and no column reserved for one.
    None,
}

impl BulletStyle {
    pub(super) fn from_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "triangle" => Some(Self::Triangle),
            "square" => Some(Self::Square),
            "diamond" => Some(Self::Diamond),
            "bevel" | "win95" | "win95square" => Some(Self::Bevel),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// Built-in theme defaults. Custom themes override this via `themes.json`.
pub(super) fn bullet_style_for_theme(theme: &str) -> BulletStyle {
    match theme {
        "grandpa" | "grandma" => BulletStyle::Bevel,
        "teletext1" | "teletext2" => BulletStyle::Square,
        "amber" | "green" => BulletStyle::Diamond,
        _ => crate::custom_themes::find_custom_theme(theme)
            .and_then(|custom| BulletStyle::from_name(custom.bullet.name()))
            .unwrap_or(BulletStyle::Triangle),
    }
}

/// Size of the mark itself, derived from the body font so it tracks font size,
/// widget scale, and DPI together. Forced odd so triangle and diamond apexes
/// land on a whole pixel instead of straddling two.
fn mark_size(tm_height: i32) -> i32 {
    let size = (tm_height * 2 / 5).max(5);
    if size % 2 == 0 {
        size + 1
    } else {
        size
    }
}

fn mark_gap(tm_height: i32) -> i32 {
    (tm_height / 3).max(4)
}

/// Width reserved at the left of every menu item line, mark plus trailing gap.
///
/// Items without a mark of their own still reserve this so wrapped rows and
/// non-primary components stay aligned under the ones that have it. A `None`
/// style reserves nothing, letting items sit flush with the headings.
pub(super) fn bullet_column_width(hdc: HDC, normal_font: HFONT, style: BulletStyle) -> i32 {
    if style == BulletStyle::None {
        return 0;
    }
    let tm_height = text_metrics(hdc, normal_font).tmHeight as i32;
    mark_size(tm_height) + mark_gap(tm_height)
}

/// Resting colour for the mark.
///
/// Bullets label the dish names rather than competing with them, so the default
/// sits back from the body text. The bevel chip is an exception: it is a
/// miniature Win95 control, so it takes the theme's button face and gets its
/// contrast from the highlight and shadow edges instead.
pub(super) fn bullet_color(style: BulletStyle, palette: &ThemePalette) -> COLORREF {
    match style {
        BulletStyle::Bevel => palette.button_bg_color,
        _ => lerp_color(palette.body_text_color, palette.bg_color, 0.35),
    }
}

/// Draws the mark, vertically centred on the text line starting at `text_top`.
pub(super) fn draw_bullet(
    hdc: HDC,
    style: BulletStyle,
    left: i32,
    text_top: i32,
    tm_height: i32,
    color: COLORREF,
) {
    if style == BulletStyle::None {
        return;
    }
    let size = mark_size(tm_height);
    let top = text_top + (tm_height - size) / 2;
    match style {
        BulletStyle::Triangle => draw_triangle(hdc, left, top, size, color),
        BulletStyle::Square => draw_square(hdc, left, top, size, color),
        BulletStyle::Diamond => draw_diamond(hdc, left, top, size, color),
        BulletStyle::Bevel => draw_bevel(hdc, left, top, size, color),
        BulletStyle::None => {}
    }
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

/// Right-pointing triangle built from scanlines, one row per pixel, so small
/// sizes stay crisp instead of picking up the antialiasing a polygon fill would.
fn draw_triangle(hdc: HDC, left: i32, top: i32, size: i32, color: COLORREF) {
    let half = size / 2;
    unsafe {
        let brush = CreateSolidBrush(color);
        for row in 0..size {
            let width = half + 1 - (row - half).abs();
            if width > 0 {
                FillRect(
                    hdc,
                    &RECT {
                        left,
                        top: top + row,
                        right: left + width,
                        bottom: top + row + 1,
                    },
                    brush,
                );
            }
        }
        DeleteObject(brush);
    }
}

/// Slightly inset from the full mark size so a solid square does not read as
/// heavier than the triangle it replaces.
fn draw_square(hdc: HDC, left: i32, top: i32, size: i32, color: COLORREF) {
    let side = (size * 3 / 4).max(3);
    let offset = (size - side) / 2;
    fill(
        hdc,
        left,
        top + offset,
        left + side,
        top + offset + side,
        color,
    );
}

fn draw_diamond(hdc: HDC, left: i32, top: i32, size: i32, color: COLORREF) {
    let half = size / 2;
    let center_x = left + half;
    unsafe {
        let brush = CreateSolidBrush(color);
        for row in 0..size {
            let reach = half - (row - half).abs();
            FillRect(
                hdc,
                &RECT {
                    left: center_x - reach,
                    top: top + row,
                    right: center_x + reach + 1,
                    bottom: top + row + 1,
                },
                brush,
            );
        }
        DeleteObject(brush);
    }
}

/// Raised chip: face, then a light top/left edge and a dark bottom/right edge.
/// Derived from the face colour so it works on any theme without extra knobs.
fn draw_bevel(hdc: HDC, left: i32, top: i32, size: i32, color: COLORREF) {
    let side = (size * 3 / 4).max(4);
    let offset = (size - side) / 2;
    let x = left;
    let y = top + offset;
    let highlight = lerp_color(color, rgb(255, 255, 255), 0.6);
    let shadow = lerp_color(color, rgb(0, 0, 0), 0.45);

    fill(hdc, x, y, x + side, y + side, color);
    fill(hdc, x, y, x + side, y + 1, highlight);
    fill(hdc, x, y, x + 1, y + side, highlight);
    fill(hdc, x, y + side - 1, x + side, y + side, shadow);
    fill(hdc, x + side - 1, y, x + side, y + side, shadow);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bullet_style_names_decode() {
        assert_eq!(
            BulletStyle::from_name("triangle"),
            Some(BulletStyle::Triangle)
        );
        assert_eq!(BulletStyle::from_name("Square"), Some(BulletStyle::Square));
        assert_eq!(
            BulletStyle::from_name(" diamond "),
            Some(BulletStyle::Diamond)
        );
        assert_eq!(
            BulletStyle::from_name("win95square"),
            Some(BulletStyle::Bevel)
        );
        assert_eq!(BulletStyle::from_name("bevel"), Some(BulletStyle::Bevel));
        assert_eq!(BulletStyle::from_name("none"), Some(BulletStyle::None));
        assert_eq!(BulletStyle::from_name("wobble"), None);
    }

    #[test]
    fn mark_size_stays_odd_for_symmetric_apexes() {
        for tm_height in 8..80 {
            assert_eq!(mark_size(tm_height) % 2, 1, "tm_height {tm_height}");
        }
    }

    #[test]
    fn mark_size_grows_with_font_height() {
        assert!(mark_size(40) > mark_size(20));
    }

    #[test]
    fn built_in_theme_defaults_cover_each_family() {
        assert_eq!(bullet_style_for_theme("grandpa"), BulletStyle::Bevel);
        assert_eq!(bullet_style_for_theme("grandma"), BulletStyle::Bevel);
        assert_eq!(bullet_style_for_theme("teletext1"), BulletStyle::Square);
        assert_eq!(bullet_style_for_theme("amber"), BulletStyle::Diamond);
        assert_eq!(bullet_style_for_theme("light"), BulletStyle::Triangle);
        assert_eq!(bullet_style_for_theme("dark"), BulletStyle::Triangle);
    }
}
