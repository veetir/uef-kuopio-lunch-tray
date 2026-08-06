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
    /// Colour of the drawn header glyphs. Separate from `body_text_color`,
    /// which is the *content* ink and only coincidentally worked on the header.
    pub(super) button_text_color: COLORREF,
    pub(super) divider_color: COLORREF,
    /// Optional fill behind every second menu group. The only structural
    /// vocabulary a theme has inside the content area — everything else there is
    /// differentiated by ink colour and font size alone.
    pub(super) group_band_color: Option<COLORREF>,
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
        // A nested console window: a shade below the page, ruled in the same
        // blue, with Cyan carrying the ingredient matches.
        "blue" => RecipeDetailPalette {
            bg_color: rgb(1, 24, 64),
            border_color: rgb(31, 77, 131),
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(0, 255, 255),
            selection_text_color: rgb(1, 36, 86),
        },
        "green" => RecipeDetailPalette {
            bg_color: rgb(4, 30, 4),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(192, 255, 192),
            selection_text_color: rgb(0, 18, 0),
        },
        "amber" => RecipeDetailPalette {
            bg_color: lerp_color(palette.bg_color, palette.selection_bg_color, 0.55),
            border_color: palette.divider_color,
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(255, 246, 166),
            selection_text_color: rgb(0, 0, 0),
        },
        // An editor's input well: darker than the page, with the comment colour
        // as its rule. Ingredient matches take Dracula yellow, which leaves the
        // pink free to mean "favourite" and nothing else.
        "dark" => RecipeDetailPalette {
            bg_color: rgb(33, 34, 44),
            border_color: rgb(98, 114, 164),
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(241, 250, 140),
            selection_text_color: rgb(40, 42, 54),
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
        // An editor widget: the sidebar grey against the white page, ruled in the
        // panel border. Ingredient matches take Light+'s function olive, which
        // leaves the string red free to mean "favourite" and nothing else —
        // the same division Dracula makes with yellow and pink.
        "light" => RecipeDetailPalette {
            bg_color: rgb(243, 243, 243),
            border_color: rgb(200, 200, 200),
            label_color: palette.heading_color,
            text_color: palette.body_text_color,
            ingredient_highlight_color: rgb(121, 94, 38),
            selection_text_color: rgb(0, 0, 0),
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
        // VS Code Light+, the counterpart to Dracula below. Text takes the Light+
        // syntax colours — #0000FF keyword, #008000 comment, #267F99 type,
        // #A31515 string — on the #FFFFFF editor background, and the chrome takes
        // its UI tokens: #ADD6FF selection, #C8C8C8 panel border, #F3F3F3 widget.
        //
        // The header is the status bar (#007ACC) rather than the title bar, which
        // is the one deliberate departure. Light+'s title bar is #DDDDDD, near
        // enough to the page that the header would lose its edge and the rail
        // markers their contrast; the status bar blue is also the colour anyone
        // would name if asked what VS Code light looks like.
        //
        // The button face is that blue darkened rather than a borrowed token:
        // Light+ sets button.background to the same #007ACC as the bar, so a
        // faithful copy would leave the buttons invisible on it.
        "light" => ThemePalette {
            bg_color: rgb(255, 255, 255),
            body_text_color: rgb(0, 0, 0),
            heading_color: rgb(0, 0, 255),
            header_title_color: rgb(255, 255, 255),
            suffix_color: rgb(0, 128, 0),
            suffix_highlight_color: rgb(38, 127, 153),
            favorite_highlight_color: rgb(163, 21, 21),
            selection_bg_color: rgb(173, 214, 255),
            header_bg_color: rgb(0, 122, 204),
            button_bg_color: rgb(0, 90, 158),
            button_text_color: rgb(255, 255, 255),
            divider_color: rgb(200, 200, 200),
            // Light+'s widget grey, the same token the recipe panel uses. The theme already says regions look like this.
            group_band_color: Some(rgb(243, 243, 243)),
            border_color: rgb(200, 200, 200),
        },
        // Dracula. Every value is from the published palette: the syntax
        // colours for text, and Dracula's own UI tokens for the chrome —
        // #21222C is its title bar, #44475A its button face, #191A21 its status
        // bar. The header sits darker than the page exactly as an editor's
        // title bar does.
        "dark" => ThemePalette {
            bg_color: rgb(40, 42, 54),
            body_text_color: rgb(248, 248, 242),
            heading_color: rgb(189, 147, 249),
            header_title_color: rgb(248, 248, 242),
            suffix_color: rgb(98, 114, 164),
            suffix_highlight_color: rgb(139, 233, 253),
            favorite_highlight_color: rgb(255, 121, 198),
            selection_bg_color: rgb(68, 71, 90),
            header_bg_color: rgb(33, 34, 44),
            button_bg_color: rgb(68, 71, 90),
            button_text_color: rgb(248, 248, 242),
            divider_color: rgb(25, 26, 33),
            // Dracula's darker background, which is also this theme's header. Recessed rather than lit, so the block reads as chrome.
            group_band_color: Some(rgb(33, 34, 44)),
            border_color: rgb(25, 26, 33),
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
            button_text_color: rgb(0, 0, 0),
            divider_color: rgb(128, 128, 128),
            // A shade toward this theme's own #808080 divider, not a white field.
            // White was tried first and read as a hole punched in the page: it
            // was the one band here that advanced instead of receding, at nearly
            // twice the strength of any other, and white already means "recipe
            // panel" in this theme. A Win95 field is only white when it is also
            // sunken, and the band carries no edge.
            group_band_color: Some(rgb(180, 180, 180)),
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
            button_text_color: rgb(52, 16, 38),
            divider_color: rgb(154, 112, 133),
            // Grandpa's move in this theme's pink: one step lighter than the page.
            group_band_color: Some(rgb(246, 214, 230)),
            border_color: rgb(154, 112, 133),
        },
        // Windows PowerShell console. The background is the exact value the
        // PowerShell shortcut remaps DarkMagenta to, and the text roles map onto
        // PSReadLine's own semantics: Yellow for commands, Cyan for emphasis,
        // Green for variables, DarkGray for parameters. Previously the heading
        // and the body were both white and the suffix and its highlight were
        // both the same blue, so neither distinction rendered at all. Surfaces
        // are derived, since a console has no widgets to borrow from.
        "blue" => ThemePalette {
            bg_color: rgb(1, 36, 86),
            body_text_color: rgb(238, 237, 240),
            heading_color: rgb(255, 255, 0),
            header_title_color: rgb(238, 237, 240),
            suffix_color: rgb(128, 128, 128),
            suffix_highlight_color: rgb(0, 255, 255),
            favorite_highlight_color: rgb(0, 255, 0),
            selection_bg_color: rgb(11, 61, 128),
            header_bg_color: rgb(1, 24, 58),
            button_bg_color: rgb(11, 61, 128),
            button_text_color: rgb(238, 237, 240),
            divider_color: rgb(31, 77, 131),
            // A shade below the page, the same relationship the recipe panel already has to it.
            group_band_color: Some(rgb(1, 24, 58)),
            border_color: rgb(31, 77, 131),
        },
        // Green phosphor terminal. A monochrome CRT has exactly one hue, so
        // every role is a brightness step on it, the way Amber below already
        // worked. Previously the heading matched the body and the suffix
        // highlight matched the body too, so neither read as emphasis, and the
        // favourite was yellow — a colour a green phosphor cannot physically
        // produce.
        "green" => ThemePalette {
            bg_color: rgb(0, 18, 0),
            body_text_color: rgb(0, 212, 0),
            heading_color: rgb(51, 255, 51),
            header_title_color: rgb(51, 255, 51),
            suffix_color: rgb(0, 154, 0),
            suffix_highlight_color: rgb(51, 255, 51),
            favorite_highlight_color: rgb(192, 255, 192),
            selection_bg_color: rgb(10, 58, 10),
            header_bg_color: rgb(5, 41, 5),
            button_bg_color: rgb(11, 74, 11),
            button_text_color: rgb(0, 212, 0),
            divider_color: rgb(31, 110, 31),
            group_band_color: None,
            border_color: rgb(31, 110, 31),
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
            // Was rgb(74, 42, 12), which sat at 1.19:1 against its own header.
            button_bg_color: rgb(110, 63, 18),
            button_text_color: rgb(255, 180, 24),
            divider_color: rgb(110, 63, 18),
            group_band_color: None,
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
            button_text_color: rgb(255, 255, 255),
            divider_color: rgb(255, 0, 0),
            // Teletext drew solid background blocks natively; a blue field behind a section is the format working as designed, not decoration added to it.
            group_band_color: Some(rgb(0, 0, 180)),
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
            button_text_color: rgb(255, 255, 255),
            divider_color: rgb(255, 0, 255),
            // The same blue block, held darker so it does not fight the green header.
            group_band_color: Some(rgb(0, 0, 128)),
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
                    button_text_color: custom.button_text_color,
                    divider_color: custom.divider_color,
                    group_band_color: custom.group_band_color,
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
                    button_text_color: COLORREF(0x00FFFFFF),
                    divider_color: COLORREF(0x00202020),
                    group_band_color: None,
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
        "amber" | "teletext1" | "teletext2" | "dark" | "blue" => "Consolas",
        "grandpa" | "grandma" => "Tahoma",
        // Consolas, matching Dark: both are editor themes now, so both take an
        // editor's font. Light used Georgia when it was a print referent.
        "light" => "Consolas",
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

    /// Text roles have to clear their contrast floors on their own background:
    /// 4.5:1 for body-size text, 3:1 for the large bold headings. Light shipped
    /// with a heading at 2.7:1 and a favourite highlight at 2.0:1 before this
    /// was checked.
    #[test]
    fn every_built_in_theme_keeps_its_text_readable() {
        for theme in BUILT_IN_THEMES {
            let p = theme_palette(theme);
            for (role, color, floor) in [
                ("body", p.body_text_color, 4.5),
                ("heading", p.heading_color, 3.0),
                // Secondary metadata: allergen codes and group captions, both
                // of which restate something the layout already shows. Held to
                // the 3:1 large-text floor rather than 4.5 so Dracula can keep
                // its own Comment colour, which is recessive by design.
                ("caption", p.suffix_color, 3.0),
                ("suffix highlight", p.suffix_highlight_color, 4.5),
                ("favorite", p.favorite_highlight_color, 4.5),
            ] {
                let ratio = contrast_ratio(color, p.bg_color);
                assert!(
                    ratio >= floor,
                    "{theme} {role} sits at {ratio:.2}:1, needs {floor}"
                );
            }
            let glyph = contrast_ratio(p.button_text_color, p.button_bg_color);
            assert!(
                glyph >= 3.0,
                "{theme} header glyphs sit at {glyph:.2}:1 on their button"
            );
        }
    }

    /// A control that barely differs from the bar it sits on is what an
    /// unstyled widget looks like. Light shipped at 1.37:1 and read as broken;
    /// Amber, Green and Blue were worse still.
    #[test]
    fn every_built_in_theme_separates_its_buttons_from_the_header() {
        for theme in BUILT_IN_THEMES {
            let p = theme_palette(theme);
            let ratio = contrast_ratio(p.button_bg_color, p.header_bg_color);
            assert!(
                ratio >= 1.4,
                "{theme} buttons sit at {ratio:.2}:1 against their own header"
            );
        }
    }

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
