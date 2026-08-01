use super::*;

pub(super) fn lerp_color(from: COLORREF, to: COLORREF, t: f32) -> COLORREF {
    let p = t.clamp(0.0, 1.0);
    let (fr, fg, fb) = color_channels(from);
    let (tr, tg, tb) = color_channels(to);
    let r = fr as f32 + (tr as f32 - fr as f32) * p;
    let g = fg as f32 + (tg as f32 - fg as f32) * p;
    let b = fb as f32 + (tb as f32 - fb as f32) * p;
    COLORREF(((b as u32) << 16) | ((g as u32) << 8) | (r as u32))
}

fn color_channels(color: COLORREF) -> (u8, u8, u8) {
    let value = color.0;
    let r = (value & 0xFF) as u8;
    let g = ((value >> 8) & 0xFF) as u8;
    let b = ((value >> 16) & 0xFF) as u8;
    (r, g, b)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ThemePalette {
    pub(super) bg_color: COLORREF,
    pub(super) body_text_color: COLORREF,
    pub(super) heading_color: COLORREF,
    pub(super) header_title_color: COLORREF,
    pub(super) suffix_color: COLORREF,
    pub(super) suffix_highlight_color: COLORREF,
    pub(super) favorite_highlight_color: COLORREF,
    pub(super) selection_bg_color: COLORREF,
    pub(super) header_bg_color: COLORREF,
    pub(super) button_bg_color: COLORREF,
    pub(super) divider_color: COLORREF,
    /// Colour of a flat chrome edge. Built-in themes reuse their divider; custom
    /// themes may name their own via `border_color` in `themes.json`.
    pub(super) border_color: COLORREF,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RecipeDetailPalette {
    pub(super) bg_color: COLORREF,
    pub(super) border_color: COLORREF,
    pub(super) label_color: COLORREF,
    pub(super) text_color: COLORREF,
    pub(super) ingredient_highlight_color: COLORREF,
    pub(super) selection_text_color: COLORREF,
}

pub(super) fn recipe_detail_palette(theme: &str, palette: &ThemePalette) -> RecipeDetailPalette {
    match theme {
        "teletext1" => RecipeDetailPalette {
            bg_color: rgb(0, 0, 120),
            border_color: rgb(255, 255, 0),
            label_color: rgb(0, 255, 255),
            text_color: rgb(255, 255, 255),
            ingredient_highlight_color: rgb(255, 255, 0),
            selection_text_color: rgb(0, 0, 0),
        },
        "teletext2" => RecipeDetailPalette {
            bg_color: rgb(0, 0, 130),
            border_color: rgb(255, 0, 255),
            label_color: rgb(255, 255, 0),
            text_color: rgb(255, 255, 255),
            ingredient_highlight_color: rgb(255, 0, 255),
            selection_text_color: rgb(0, 0, 0),
        },
        "blue" => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.55),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(255, 211, 80),
            selection_text_color: rgb(0, 0, 0),
        },
        "green" => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.55),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(255, 255, 0),
            selection_text_color: rgb(0, 0, 0),
        },
        "amber" => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.55),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(255, 246, 166),
            selection_text_color: rgb(0, 0, 0),
        },
        "grandpa" => RecipeDetailPalette {
            bg_color: rgb(255, 255, 255),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: palette.favorite_highlight_color,
            selection_text_color: rgb(255, 255, 255),
        },
        // Tinted rather than white: the classic idiom fills a well with the
        // lightest tone available, but that was a small control on a grey page.
        // Here the panel is the largest block on screen, so pure white glares
        // against the pink face instead of reading as a recessed well.
        "grandma" => RecipeDetailPalette {
            bg_color: rgb(248, 233, 242),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: palette.favorite_highlight_color,
            selection_text_color: rgb(255, 255, 255),
        },
        "light" => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.38),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(32, 92, 176),
            selection_text_color: rgb(255, 255, 255),
        },
        _ => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.55),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: palette.favorite_highlight_color,
            selection_text_color: rgb(0, 0, 0),
        },
    }
}

/// Minimum contrast between an inactive rail marker and the header behind it.
///
/// The markers carry count and position, which makes them meaningful non-text
/// graphics, and 3:1 is the WCAG 1.4.11 threshold for those. Enforced as a floor
/// rather than a target: the derivation below aims a little above it, and this
/// catches the palettes where that lands short.
pub(super) const MARKER_MIN_CONTRAST: f32 = 3.0;

fn relative_luminance(color: COLORREF) -> f32 {
    fn channel(value: u8) -> f32 {
        let c = value as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let (r, g, b) = color_channels(color);
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// WCAG contrast ratio between two colors, from 1.0 to 21.0.
pub(super) fn contrast_ratio(a: COLORREF, b: COLORREF) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Pushes `fg` toward white or black, whichever gains contrast against `bg`,
/// until it clears `min_ratio`. Returns `fg` untouched when it already does.
pub(super) fn ensure_min_contrast(fg: COLORREF, bg: COLORREF, min_ratio: f32) -> COLORREF {
    if contrast_ratio(fg, bg) >= min_ratio {
        return fg;
    }
    // Measure both directions rather than guessing from luminance: a
    // mid-luminance background like Teletext 2's green sits just under 0.5 yet
    // gains far more contrast from black (10.7:1) than from white (2.0:1).
    let white = rgb(255, 255, 255);
    let black = rgb(0, 0, 0);
    let target = if contrast_ratio(white, bg) >= contrast_ratio(black, bg) {
        white
    } else {
        black
    };
    let steps = 24;
    let mut adjusted = fg;
    for step in 1..=steps {
        adjusted = lerp_color(fg, target, step as f32 / steps as f32);
        if contrast_ratio(adjusted, bg) >= min_ratio {
            break;
        }
    }
    adjusted
}

/// Color of the inactive markers in the header restaurant rail.
///
/// Mixed from the header background toward the header's own title color rather
/// than from `divider_color`: the rail lives in the header, so its palette
/// should come from there. The old divider mix produced maroon markers on
/// Teletext's blue header and a near-invisible pink on Grandma's.
pub(super) fn marker_inactive_color(palette: &ThemePalette) -> COLORREF {
    let mixed = lerp_color(palette.header_bg_color, palette.header_title_color, 0.45);
    ensure_min_contrast(mixed, palette.header_bg_color, MARKER_MIN_CONTRAST)
}

pub(super) fn theme_palette(theme: &str) -> ThemePalette {
    match theme {
        "light" => ThemePalette {
            bg_color: rgb(245, 251, 252),
            body_text_color: rgb(31, 41, 51),
            heading_color: rgb(31, 167, 200),
            header_title_color: rgb(255, 255, 255),
            suffix_color: rgb(107, 124, 133),
            suffix_highlight_color: rgb(21, 149, 181),
            favorite_highlight_color: rgb(39, 196, 216),
            selection_bg_color: rgb(217, 243, 248),
            header_bg_color: rgb(21, 149, 181),
            button_bg_color: rgb(31, 167, 200),
            divider_color: rgb(127, 211, 226),
            border_color: rgb(127, 211, 226),
        },
        "dark" => ThemePalette {
            bg_color: rgb(40, 42, 54),
            body_text_color: rgb(248, 248, 242),
            heading_color: rgb(189, 147, 249),
            header_title_color: rgb(248, 248, 242),
            suffix_color: rgb(98, 114, 164),
            suffix_highlight_color: rgb(139, 233, 253),
            favorite_highlight_color: rgb(255, 121, 198),
            selection_bg_color: rgb(68, 71, 90),
            header_bg_color: rgb(68, 71, 90),
            button_bg_color: rgb(59, 61, 82),
            divider_color: rgb(98, 114, 164),
            border_color: rgb(98, 114, 164),
        },
        "grandpa" => ThemePalette {
            bg_color: rgb(192, 192, 192),
            body_text_color: rgb(0, 0, 0),
            heading_color: rgb(0, 0, 128),
            header_title_color: rgb(255, 255, 255),
            suffix_color: rgb(64, 64, 64),
            suffix_highlight_color: rgb(0, 0, 128),
            favorite_highlight_color: rgb(128, 0, 0),
            selection_bg_color: rgb(212, 208, 200),
            header_bg_color: rgb(0, 0, 128),
            button_bg_color: rgb(184, 184, 184),
            divider_color: rgb(128, 128, 128),
            border_color: rgb(128, 128, 128),
        },
        // Grandpa's structure in pink. The face is deliberately mid-toned
        // rather than the near-white pink it would otherwise want: a raised
        // bevel needs room for both a white highlight and a black shadow, which
        // is exactly why the original face grey sat at 192.
        "grandma" => ThemePalette {
            bg_color: rgb(231, 188, 212),
            body_text_color: rgb(52, 16, 38),
            heading_color: rgb(158, 16, 96),
            header_title_color: rgb(255, 255, 255),
            suffix_color: rgb(122, 78, 100),
            suffix_highlight_color: rgb(158, 16, 96),
            favorite_highlight_color: rgb(104, 34, 148),
            selection_bg_color: rgb(246, 214, 230),
            header_bg_color: rgb(168, 20, 104),
            button_bg_color: rgb(223, 178, 203),
            divider_color: rgb(154, 112, 133),
            border_color: rgb(154, 112, 133),
        },
        "blue" => ThemePalette {
            bg_color: COLORREF(0x00562401),
            body_text_color: COLORREF(0x00FFFFFF),
            heading_color: COLORREF(0x00FFFFFF),
            header_title_color: COLORREF(0x00FFFFFF),
            suffix_color: COLORREF(0x00E7C7A7),
            suffix_highlight_color: COLORREF(0x00E7C7A7),
            favorite_highlight_color: COLORREF(0x0000D6FF),
            selection_bg_color: COLORREF(0x003E2B1A),
            header_bg_color: COLORREF(0x00733809),
            button_bg_color: COLORREF(0x00804A1A),
            divider_color: COLORREF(0x00834D1F),
            border_color: COLORREF(0x00834D1F),
        },
        "green" => ThemePalette {
            bg_color: COLORREF(0x00000000),
            body_text_color: COLORREF(0x0000D000),
            heading_color: COLORREF(0x0000D000),
            header_title_color: COLORREF(0x0000D000),
            suffix_color: COLORREF(0x00009000),
            suffix_highlight_color: COLORREF(0x0000D000),
            favorite_highlight_color: COLORREF(0x0000FFFF),
            selection_bg_color: COLORREF(0x001A2F1A),
            header_bg_color: COLORREF(0x000B1A0B),
            button_bg_color: COLORREF(0x00142D14),
            divider_color: COLORREF(0x00142D14),
            border_color: COLORREF(0x00142D14),
        },
        "amber" => ThemePalette {
            bg_color: rgb(26, 16, 6),
            body_text_color: rgb(255, 180, 24),
            heading_color: rgb(255, 198, 72),
            header_title_color: rgb(255, 207, 92),
            suffix_color: rgb(194, 120, 24),
            suffix_highlight_color: rgb(255, 224, 120),
            favorite_highlight_color: rgb(255, 246, 166),
            selection_bg_color: rgb(82, 45, 8),
            header_bg_color: rgb(56, 31, 9),
            button_bg_color: rgb(74, 42, 12),
            divider_color: rgb(110, 63, 18),
            border_color: rgb(110, 63, 18),
        },
        "teletext1" => ThemePalette {
            bg_color: rgb(0, 0, 0),
            body_text_color: rgb(255, 255, 255),
            heading_color: rgb(0, 255, 255),
            header_title_color: rgb(255, 255, 0),
            suffix_color: rgb(0, 255, 0),
            suffix_highlight_color: rgb(255, 0, 255),
            favorite_highlight_color: rgb(255, 255, 0),
            selection_bg_color: rgb(0, 0, 180),
            header_bg_color: rgb(0, 0, 180),
            // Lighter than the header bar, not darker: a face darker than its
            // bar reads as recessed, and pure black read as a hole punched
            // through to the page, which is black in this theme.
            button_bg_color: rgb(48, 48, 208),
            divider_color: rgb(255, 0, 0),
            border_color: rgb(255, 0, 0),
        },
        "teletext2" => ThemePalette {
            bg_color: rgb(0, 0, 0),
            body_text_color: rgb(255, 255, 255),
            heading_color: rgb(255, 0, 255),
            // Level 1 teletext only ever used the eight full-saturation RGB
            // corners, so every text role here is one of them. The previous
            // #0060FF title also sat at 2.6:1 on this theme's green header.
            header_title_color: rgb(0, 0, 255),
            suffix_color: rgb(0, 255, 0),
            suffix_highlight_color: rgb(255, 255, 0),
            favorite_highlight_color: rgb(0, 255, 255),
            selection_bg_color: rgb(0, 96, 255),
            header_bg_color: rgb(0, 215, 0),
            button_bg_color: rgb(0, 145, 0),
            divider_color: rgb(255, 0, 255),
            border_color: rgb(255, 0, 255),
        },
        _ => {
            if let Some(custom) = crate::custom_themes::find_custom_theme(theme) {
                ThemePalette {
                    bg_color: custom.bg_color,
                    body_text_color: custom.body_text_color,
                    heading_color: custom.heading_color,
                    header_title_color: custom.header_title_color,
                    suffix_color: custom.suffix_color,
                    suffix_highlight_color: custom.suffix_highlight_color,
                    favorite_highlight_color: custom.favorite_highlight_color,
                    selection_bg_color: custom.selection_bg_color,
                    header_bg_color: custom.header_bg_color,
                    button_bg_color: custom.button_bg_color,
                    divider_color: custom.divider_color,
                    border_color: custom.divider_color,
                }
            } else {
                ThemePalette {
                    bg_color: COLORREF(0x00000000),
                    body_text_color: COLORREF(0x00FFFFFF),
                    heading_color: COLORREF(0x00FFFFFF),
                    header_title_color: COLORREF(0x00FFFFFF),
                    suffix_color: COLORREF(0x00B0B0B0),
                    suffix_highlight_color: COLORREF(0x00B0B0B0),
                    favorite_highlight_color: COLORREF(0x0000D6FF),
                    selection_bg_color: COLORREF(0x00303030),
                    header_bg_color: COLORREF(0x00101010),
                    button_bg_color: COLORREF(0x00202020),
                    divider_color: COLORREF(0x00202020),
                    border_color: COLORREF(0x00202020),
                }
            }
        }
    }
}

pub(super) fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

pub(super) fn theme_font_family(theme: &str) -> &'static str {
    match theme {
        "amber" | "teletext1" | "teletext2" => "Consolas",
        "grandpa" | "grandma" => "Tahoma",
        _ => crate::custom_themes::find_custom_theme(theme)
            .map(|custom| custom.font.family())
            .unwrap_or("Segoe UI"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every theme offered in the tray menu.
    const BUILT_IN_THEMES: &[&str] = &[
        "light",
        "dark",
        "blue",
        "green",
        "amber",
        "teletext1",
        "teletext2",
        "grandpa",
        "grandma",
    ];

    #[test]
    fn contrast_ratio_spans_the_full_range() {
        let white = rgb(255, 255, 255);
        let black = rgb(0, 0, 0);

        assert!((contrast_ratio(white, black) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 0.01);
    }

    #[test]
    fn ensure_min_contrast_lifts_a_failing_pair() {
        let bg = rgb(168, 20, 104);
        let fg = rgb(162, 61, 117);
        assert!(contrast_ratio(fg, bg) < MARKER_MIN_CONTRAST);

        let fixed = ensure_min_contrast(fg, bg, MARKER_MIN_CONTRAST);
        assert!(contrast_ratio(fixed, bg) >= MARKER_MIN_CONTRAST);
    }

    #[test]
    fn ensure_min_contrast_leaves_a_passing_pair_alone() {
        let bg = rgb(0, 0, 0);
        let fg = rgb(255, 255, 255);

        assert_eq!(ensure_min_contrast(fg, bg, MARKER_MIN_CONTRAST).0, fg.0);
    }

    /// Guards the rail against a palette edit quietly making the inactive
    /// markers unreadable again, which is how Grandma shipped the first time.
    #[test]
    fn every_built_in_theme_keeps_its_rail_markers_visible() {
        for theme in BUILT_IN_THEMES {
            let palette = theme_palette(theme);
            let ratio = contrast_ratio(marker_inactive_color(&palette), palette.header_bg_color);
            assert!(
                ratio >= MARKER_MIN_CONTRAST,
                "{theme} inactive markers sit at {ratio:.2}:1"
            );
        }
    }

    #[test]
    fn active_rail_markers_stay_readable_too() {
        for theme in BUILT_IN_THEMES {
            let palette = theme_palette(theme);
            let ratio = contrast_ratio(palette.header_title_color, palette.header_bg_color);
            assert!(
                ratio >= MARKER_MIN_CONTRAST,
                "{theme} active marker sits at {ratio:.2}:1"
            );
        }
    }
}
