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
            text_color: rgb(225, 255, 225),
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
        "light" => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.38),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(32, 92, 176),
            selection_text_color: rgb(255, 255, 255),
        },
        "barbie" => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.38),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(210, 40, 135),
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
        },
        "barbie" => ThemePalette {
            bg_color: rgb(255, 243, 248),
            body_text_color: rgb(90, 45, 73),
            heading_color: rgb(216, 27, 138),
            header_title_color: rgb(255, 255, 255),
            suffix_color: rgb(168, 107, 145),
            suffix_highlight_color: rgb(194, 24, 91),
            favorite_highlight_color: rgb(255, 105, 180),
            selection_bg_color: rgb(255, 217, 235),
            header_bg_color: rgb(236, 74, 168),
            button_bg_color: rgb(240, 98, 179),
            divider_color: rgb(245, 163, 204),
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
            button_bg_color: rgb(0, 0, 140),
            divider_color: rgb(255, 0, 0),
        },
        "teletext2" => ThemePalette {
            bg_color: rgb(0, 0, 0),
            body_text_color: rgb(225, 255, 225),
            heading_color: rgb(255, 0, 255),
            header_title_color: rgb(0, 96, 255),
            suffix_color: rgb(0, 255, 150),
            suffix_highlight_color: rgb(255, 255, 0),
            favorite_highlight_color: rgb(0, 255, 255),
            selection_bg_color: rgb(0, 96, 255),
            header_bg_color: rgb(0, 215, 0),
            button_bg_color: rgb(0, 145, 0),
            divider_color: rgb(255, 0, 255),
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
        "grandpa" => "Tahoma",
        _ => crate::custom_themes::find_custom_theme(theme)
            .map(|custom| custom.font.family())
            .unwrap_or("Segoe UI"),
    }
}
