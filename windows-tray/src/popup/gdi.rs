//! Instrumented Win32/GDI entry points used by popup painting.
//!
//! Keeping the counters at this boundary records executed calls, including calls
//! made from loops, without scattering feature gates through the renderer.

#![allow(non_snake_case)]

use crate::perf::{count_gdi_call, GdiCall};
use std::ffi::c_void;
use windows::core::{IntoParam, PCWSTR};
use windows::Win32::Foundation::{BOOL, COLORREF, HWND, RECT, SIZE};
use windows::Win32::Graphics::Gdi as raw;
use windows::Win32::Graphics::Gdi::{
    BACKGROUND_MODE, GDI_REGION_TYPE, GET_DEVICE_CAPS_INDEX, HBITMAP, HBRUSH, HDC, HFONT, HGDIOBJ,
    PAINTSTRUCT, ROP_CODE, TEXTMETRICW,
};

#[inline(always)]
pub(super) unsafe fn BeginPaint(hwnd: HWND, paint: *mut PAINTSTRUCT) -> HDC {
    count_gdi_call(GdiCall::BeginPaint);
    raw::BeginPaint(hwnd, paint)
}

#[inline(always)]
pub(super) unsafe fn EndPaint(hwnd: HWND, paint: *const PAINTSTRUCT) -> BOOL {
    count_gdi_call(GdiCall::EndPaint);
    raw::EndPaint(hwnd, paint)
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) unsafe fn BitBlt(
    hdc: HDC,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    source: HDC,
    source_x: i32,
    source_y: i32,
    rop: ROP_CODE,
) -> windows::core::Result<()> {
    count_gdi_call(GdiCall::BitBlt);
    raw::BitBlt(hdc, x, y, width, height, source, source_x, source_y, rop)
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) unsafe fn MaskBlt(
    destination: HDC,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    source: HDC,
    source_x: i32,
    source_y: i32,
    mask: HBITMAP,
    mask_x: i32,
    mask_y: i32,
    rop: u32,
) -> BOOL {
    count_gdi_call(GdiCall::MaskBlt);
    raw::MaskBlt(
        destination,
        x,
        y,
        width,
        height,
        source,
        source_x,
        source_y,
        mask,
        mask_x,
        mask_y,
        rop,
    )
}

#[inline(always)]
pub(super) unsafe fn CreateBitmap(
    width: i32,
    height: i32,
    planes: u32,
    bits_per_pixel: u32,
    bits: Option<*const c_void>,
) -> HBITMAP {
    count_gdi_call(GdiCall::CreateBitmap);
    raw::CreateBitmap(width, height, planes, bits_per_pixel, bits)
}

#[inline(always)]
pub(super) unsafe fn CreateCompatibleBitmap(hdc: HDC, width: i32, height: i32) -> HBITMAP {
    count_gdi_call(GdiCall::CreateCompatibleBitmap);
    raw::CreateCompatibleBitmap(hdc, width, height)
}

#[inline(always)]
pub(super) unsafe fn CreateCompatibleDC(hdc: HDC) -> HDC {
    count_gdi_call(GdiCall::CreateCompatibleDc);
    raw::CreateCompatibleDC(hdc)
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub(super) unsafe fn CreateFontW(
    height: i32,
    width: i32,
    escapement: i32,
    orientation: i32,
    weight: i32,
    italic: u32,
    underline: u32,
    strikeout: u32,
    charset: u32,
    output_precision: u32,
    clip_precision: u32,
    quality: u32,
    pitch_and_family: u32,
    face_name: PCWSTR,
) -> HFONT {
    count_gdi_call(GdiCall::CreateFont);
    raw::CreateFontW(
        height,
        width,
        escapement,
        orientation,
        weight,
        italic,
        underline,
        strikeout,
        charset,
        output_precision,
        clip_precision,
        quality,
        pitch_and_family,
        face_name,
    )
}

#[inline(always)]
pub(super) unsafe fn CreateSolidBrush(color: COLORREF) -> HBRUSH {
    count_gdi_call(GdiCall::CreateSolidBrush);
    raw::CreateSolidBrush(color)
}

#[inline(always)]
pub(super) unsafe fn DeleteDC(hdc: HDC) -> BOOL {
    count_gdi_call(GdiCall::DeleteDc);
    raw::DeleteDC(hdc)
}

#[inline(always)]
pub(super) unsafe fn DeleteObject<P0>(object: P0) -> BOOL
where
    P0: IntoParam<HGDIOBJ>,
{
    count_gdi_call(GdiCall::DeleteObject);
    raw::DeleteObject(object)
}

#[inline(always)]
pub(super) unsafe fn FillRect(hdc: HDC, rect: *const RECT, brush: HBRUSH) -> i32 {
    count_gdi_call(GdiCall::FillRect);
    raw::FillRect(hdc, rect, brush)
}

#[inline(always)]
pub(super) unsafe fn GetDeviceCaps(hdc: HDC, index: GET_DEVICE_CAPS_INDEX) -> i32 {
    count_gdi_call(GdiCall::GetDeviceCaps);
    raw::GetDeviceCaps(hdc, index)
}

#[inline(always)]
pub(super) unsafe fn GetTextExtentPoint32W(hdc: HDC, text: &[u16], size: *mut SIZE) -> BOOL {
    count_gdi_call(GdiCall::GetTextExtent);
    raw::GetTextExtentPoint32W(hdc, text, size)
}

#[inline(always)]
pub(super) unsafe fn GetTextMetricsW(hdc: HDC, metrics: *mut TEXTMETRICW) -> BOOL {
    count_gdi_call(GdiCall::GetTextMetrics);
    raw::GetTextMetricsW(hdc, metrics)
}

#[inline(always)]
pub(super) unsafe fn IntersectClipRect(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> GDI_REGION_TYPE {
    count_gdi_call(GdiCall::IntersectClipRect);
    raw::IntersectClipRect(hdc, left, top, right, bottom)
}

#[inline(always)]
pub(super) unsafe fn RestoreDC(hdc: HDC, saved_dc: i32) -> BOOL {
    count_gdi_call(GdiCall::RestoreDc);
    raw::RestoreDC(hdc, saved_dc)
}

#[inline(always)]
pub(super) unsafe fn SaveDC(hdc: HDC) -> i32 {
    count_gdi_call(GdiCall::SaveDc);
    raw::SaveDC(hdc)
}

#[inline(always)]
pub(super) unsafe fn SelectObject<P0>(hdc: HDC, object: P0) -> HGDIOBJ
where
    P0: IntoParam<HGDIOBJ>,
{
    count_gdi_call(GdiCall::SelectObject);
    raw::SelectObject(hdc, object)
}

#[inline(always)]
pub(super) unsafe fn SetBkMode(hdc: HDC, mode: BACKGROUND_MODE) -> i32 {
    count_gdi_call(GdiCall::SetBkMode);
    raw::SetBkMode(hdc, mode)
}

#[inline(always)]
pub(super) unsafe fn SetTextColor(hdc: HDC, color: COLORREF) -> COLORREF {
    count_gdi_call(GdiCall::SetTextColor);
    raw::SetTextColor(hdc, color)
}

#[inline(always)]
pub(super) unsafe fn TextOutW(hdc: HDC, x: i32, y: i32, text: &[u16]) -> BOOL {
    count_gdi_call(GdiCall::TextOut);
    raw::TextOutW(hdc, x, y, text)
}
