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

pub(super) fn theme_palette(theme: &str) -> ThemePalette {
    match theme {
        "light" => ThemePalette {
            bg_color: COLORREF(0x00FFFFFF),
            body_text_color: COLORREF(0x00000000),
            heading_color: COLORREF(0x00000000),
            header_title_color: COLORREF(0x00000000),
            suffix_color: COLORREF(0x00808080),
            suffix_highlight_color: COLORREF(0x00808080),
            favorite_highlight_color: COLORREF(0x00996600),
            selection_bg_color: COLORREF(0x00CDEBFF),
            header_bg_color: COLORREF(0x00F3F3F3),
            button_bg_color: COLORREF(0x00DDDDDD),
            divider_color: COLORREF(0x00C9C9C9),
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
            bg_color: rgb(245, 225, 236),
            body_text_color: rgb(88, 28, 76),
            heading_color: rgb(216, 38, 131),
            header_title_color: rgb(255, 255, 255),
            suffix_color: rgb(182, 106, 153),
            suffix_highlight_color: rgb(210, 40, 135),
            favorite_highlight_color: rgb(54, 104, 211),
            selection_bg_color: rgb(232, 195, 221),
            header_bg_color: rgb(230, 64, 148),
            button_bg_color: rgb(236, 138, 208),
            divider_color: rgb(198, 43, 117),
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
        _ => ThemePalette {
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
        },
    }
}

pub(super) fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

pub(super) fn theme_font_family(theme: &str) -> &'static str {
    match theme {
        "amber" | "teletext1" | "teletext2" => "Consolas",
        _ => "Segoe UI",
    }
}
