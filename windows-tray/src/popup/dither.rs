//! Ordered-dither vocabulary, used by the header marker rail.
//!
//! The rail draws a comet trail behind the active marker when the user scrolls
//! quickly through restaurants, and the ghosts in that trail are stippled rather
//! than drawn in a dimmer colour. A dimmed ghost competes with the real marker
//! for which restaurant you are on; a stippled one cannot be mistaken for solid
//! at any density, so the rail stays unambiguous while still showing motion.
//!
//! Deliberately not applied to text. The title has to stay crisp and readable at
//! all times, so the motion lives on the graphic that can carry it for free.
//!
//! The pattern is device-pixel art and is not scaled with DPI. It is read as
//! coverage rather than as texture, so a finer pattern at higher DPI is the
//! harmless direction: it resolves toward a smooth fade instead of breaking up
//! into visible checkerboard.

use super::*;
use windows::Win32::Graphics::Gdi::{CreateBitmap, MaskBlt};

/// The classic 8x8 ordered dither matrix, holding each threshold 0..=63 once.
/// Its recursive construction is what spreads consecutive thresholds as far
/// apart as possible, so the stipple grows evenly instead of filling in corners.
const BAYER_8X8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Coverage is expressed in thresholds: 0 shows nothing, `DITHER_STEPS` shows
/// every pixel.
pub(super) const DITHER_STEPS: u32 = 64;

/// Turns a 0.0..=1.0 progress into a coverage level.
pub(super) fn coverage_for_progress(progress: f32) -> u32 {
    (progress.clamp(0.0, 1.0) * DITHER_STEPS as f32).round() as u32
}

/// Packs a `width` x `height` 1bpp mask with a set bit wherever the ordered
/// dither admits the pixel at `coverage`.
///
/// The bits are built here rather than tiled in with a pattern brush on purpose.
/// Monochrome GDI objects map bit 0 to the text colour and bit 1 to the
/// background colour, which is easy to get backwards, and getting it backwards
/// inverts the dissolve rather than failing loudly. Computing the bits directly
/// leaves nothing to invert and makes the result testable off-Windows.
///
/// Rows are padded to the WORD stride `CreateBitmap` expects, and the leftmost
/// pixel of a row is the most significant bit of its first byte.
pub(super) fn dither_mask_bits(width: i32, height: i32, coverage: u32) -> Vec<u8> {
    let width = width.max(0) as usize;
    let height = height.max(0) as usize;
    let stride = width.div_ceil(16) * 2;
    let mut bits = vec![0u8; stride * height];
    if coverage == 0 {
        return bits;
    }
    for y in 0..height {
        for x in 0..width {
            if u32::from(BAYER_8X8[y % 8][x % 8]) < coverage {
                bits[y * stride + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    bits
}

/// `MAKEROP4`, which pairs the raster op used where the mask bit is set with the
/// one used where it is clear.
const fn make_rop4(foreground: u32, background: u32) -> u32 {
    ((background << 8) & 0xFF00_0000) | foreground
}

/// Copy the rendered text where the dither admits it, leave the destination
/// alone everywhere else. `0x00AA0029` is the "D" raster op, meaning no-op.
const DITHER_ROP4: u32 = make_rop4(0x00CC_0020, 0x00AA_0029);

/// Paints an offscreen slip and lets only the dithered pixels of it through.
///
/// Every GDI object is created and destroyed within the call. These slips are
/// marker-sized rather than window-sized, so allocating one per frame costs far
/// less than the risk of caching handles across paints.
///
/// # Safety
/// `hdc` must be a valid device context for the duration of the call.
unsafe fn blit_through_dither(
    hdc: HDC,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    coverage: u32,
    paint_slip: impl FnOnce(HDC, &RECT),
) {
    let slip_dc = CreateCompatibleDC(hdc);
    if slip_dc.0 == 0 {
        return;
    }
    let slip = CreateCompatibleBitmap(hdc, width, height);
    if slip.0 == 0 {
        let _ = DeleteDC(slip_dc);
        return;
    }
    let mask_bits = dither_mask_bits(width, height, coverage);
    let mask = CreateBitmap(width, height, 1, 1, Some(mask_bits.as_ptr().cast()));
    if mask.0 == 0 {
        let _ = DeleteObject(slip);
        let _ = DeleteDC(slip_dc);
        return;
    }

    let previous_bitmap = SelectObject(slip_dc, slip);
    let slip_rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    paint_slip(slip_dc, &slip_rect);
    let _ = MaskBlt(
        hdc,
        x,
        y,
        width,
        height,
        slip_dc,
        0,
        0,
        mask,
        0,
        0,
        DITHER_ROP4,
    );

    SelectObject(slip_dc, previous_bitmap);
    let _ = DeleteObject(mask);
    let _ = DeleteObject(slip);
    let _ = DeleteDC(slip_dc);
}

/// Fills `rect` with `color`, keeping only the pixels the dither admits.
///
/// Unlike the text path there is no background to match: pixels the mask rejects
/// are simply left alone, so whatever was already there shows between the
/// stipple. That is what lets a trail marker read as a ghost of the active one
/// rather than as a solid block in a lighter colour.
pub(super) fn fill_dithered_rect(hdc: HDC, rect: &RECT, color: COLORREF, coverage: u32) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 || coverage == 0 {
        return;
    }
    unsafe {
        if coverage >= DITHER_STEPS {
            let brush = CreateSolidBrush(color);
            FillRect(hdc, rect, brush);
            let _ = DeleteObject(brush);
            return;
        }
        blit_through_dither(
            hdc,
            rect.left,
            rect.top,
            width,
            height,
            coverage,
            |slip_dc, slip_rect| {
                let brush = CreateSolidBrush(color);
                FillRect(slip_dc, slip_rect, brush);
                let _ = DeleteObject(brush);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bayer_matrix_holds_every_threshold_once() {
        let mut seen = [false; 64];
        for row in BAYER_8X8 {
            for value in row {
                assert!(!seen[value as usize], "threshold {value} repeats");
                seen[value as usize] = true;
            }
        }
        assert!(seen.iter().all(|hit| *hit));
    }

    fn set_bits(width: i32, height: i32, coverage: u32) -> usize {
        dither_mask_bits(width, height, coverage)
            .iter()
            .map(|byte| byte.count_ones() as usize)
            .sum()
    }

    #[test]
    fn coverage_endpoints_clear_and_fill_the_mask() {
        assert_eq!(set_bits(8, 8, 0), 0);
        assert_eq!(set_bits(8, 8, DITHER_STEPS), 64);
    }

    /// The dissolve reads as even growth only if each step adds pixels and none
    /// are ever taken away, which holds exactly because the matrix is a
    /// permutation of the thresholds.
    #[test]
    fn coverage_grows_one_pixel_per_step_over_a_tile() {
        for coverage in 0..=DITHER_STEPS {
            assert_eq!(set_bits(8, 8, coverage), coverage as usize);
        }
    }

    /// Rows are padded to a WORD, so a 20px wide mask needs 4 bytes per row and
    /// the padding must never be mistaken for visible pixels.
    #[test]
    fn rows_are_word_padded_and_padding_stays_clear() {
        let bits = dither_mask_bits(20, 2, DITHER_STEPS);
        assert_eq!(bits.len(), 8);
        for row in 0..2 {
            assert_eq!(bits[row * 4], 0xFF);
            assert_eq!(bits[row * 4 + 1], 0xFF);
            assert_eq!(bits[row * 4 + 2], 0xF0, "pixels 16..20 only");
            assert_eq!(bits[row * 4 + 3], 0x00, "stride padding stays clear");
        }
    }

    #[test]
    fn degenerate_sizes_produce_no_bits() {
        assert!(dither_mask_bits(0, 10, DITHER_STEPS).is_empty());
        assert!(dither_mask_bits(-4, 10, DITHER_STEPS).is_empty());
        assert!(dither_mask_bits(10, 0, DITHER_STEPS).is_empty());
    }

    #[test]
    fn progress_maps_onto_the_full_coverage_range() {
        assert_eq!(coverage_for_progress(0.0), 0);
        assert_eq!(coverage_for_progress(1.0), DITHER_STEPS);
        assert_eq!(coverage_for_progress(0.5), DITHER_STEPS / 2);
        assert_eq!(coverage_for_progress(-1.0), 0);
        assert_eq!(coverage_for_progress(2.0), DITHER_STEPS);
    }

    /// `MAKEROP4` packs the mask-clear op into the top byte and keeps the
    /// mask-set op whole. Getting this wrong silently paints the wrong half.
    #[test]
    fn rop4_packs_both_raster_ops() {
        assert_eq!(DITHER_ROP4, 0xAACC_0020);
    }
}
