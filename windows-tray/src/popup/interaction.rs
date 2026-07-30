use super::content::invalidate_favorites_cache;
use super::layout::{header_layout, header_marker_rects, popup_scale_for_dpi};
use super::*;

const CLICK_SLOP_PX: i32 = 3;

pub(super) fn header_button_at(
    hwnd: HWND,
    settings: &Settings,
    x: i32,
    y: i32,
) -> Option<HeaderButtonAction> {
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return None;
        }
        let width = rect.right - rect.left;
        let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);
        let scale = popup_scale_for_dpi(settings, dpi_y);
        let layout = header_layout(width, &scale);
        if point_in_rect(&layout.prev, x, y) {
            return Some(HeaderButtonAction::Prev);
        }
        if point_in_rect(&layout.next, x, y) {
            return Some(HeaderButtonAction::Next);
        }
        if point_in_rect(&layout.close, x, y) {
            return Some(HeaderButtonAction::Close);
        }
        None
    }
}

pub(super) fn header_marker_index_at(
    hwnd: HWND,
    settings: &Settings,
    x: i32,
    y: i32,
) -> Option<usize> {
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return None;
        }
        let width = rect.right - rect.left;
        let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);
        windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);
        let scale = popup_scale_for_dpi(settings, dpi_y);
        header_marker_rects(width, settings, &scale)
            .iter()
            .position(|marker| point_in_rect(marker, x, y))
    }
}

pub(super) fn begin_text_selection(hwnd: HWND, x: i32, y: i32) -> bool {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let layout = match state.layout.as_ref() {
        Some(value) if value.hwnd == hwnd => value,
        _ => return false,
    };
    let Some((row, anchor_index)) = hit_test_row(layout, x, y) else {
        return false;
    };
    state.drag = Some(SelectionDrag {
        item_id: row.item_id,
        anchor: anchor_index,
        current: anchor_index,
        start_x: x,
        start_y: y,
    });
    request_repaint(hwnd);
    true
}

pub(super) fn update_text_selection(hwnd: HWND, x: i32, y: i32) {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    let Some(drag) = state.drag.as_ref() else {
        return;
    };
    let item_id = drag.item_id;
    let next_index = match state.layout.as_ref() {
        Some(value) if value.hwnd == hwnd => {
            let Some((_, next_index)) = hit_test_row_for_item(value, item_id, x, y) else {
                return;
            };
            next_index
        }
        _ => return,
    };
    let Some(drag) = state.drag.as_mut() else {
        return;
    };
    if drag.current != next_index {
        drag.current = next_index;
        request_repaint(hwnd);
    }
}

pub(super) fn finish_text_selection(hwnd: HWND, x: i32, y: i32) -> bool {
    let outcome = {
        let mut state = match selection_state().lock() {
            Ok(value) => value,
            Err(_) => return false,
        };
        let layout = match state.layout.as_ref() {
            Some(value) if value.hwnd == hwnd => value.clone(),
            _ => return false,
        };
        let Some(mut drag) = state.drag.take() else {
            return false;
        };
        if let Some((_, next_index)) = hit_test_row_for_item(&layout, drag.item_id, x, y) {
            drag.current = next_index;
        }
        let selected = selected_range(drag.anchor, drag.current);
        let is_click =
            (x - drag.start_x).abs() <= CLICK_SLOP_PX && (y - drag.start_y).abs() <= CLICK_SLOP_PX;
        if is_click || selected.0 == selected.1 {
            if layout
                .item_ingredient_flags
                .get(drag.item_id)
                .copied()
                .unwrap_or(false)
            {
                None
            } else {
                let recipe_key = layout.item_recipe_keys.get(drag.item_id).copied().flatten();
                if let Some(recipe_key) = recipe_key {
                    state.expanded_recipe_key = if state.expanded_recipe_key == Some(recipe_key) {
                        None
                    } else {
                        Some(recipe_key)
                    };
                    state.recipe_scroll_offset_px = 0;
                    Some(TextInteractionOutcome::ToggleRecipe)
                } else {
                    None
                }
            }
        } else {
            let item = layout.items.get(drag.item_id);
            let selected_text = item.and_then(|text| {
                text.get(selected.0..selected.1)
                    .map(favorites::normalize_snippet)
                    .filter(|value| !value.is_empty())
            });
            if layout
                .item_ingredient_flags
                .get(drag.item_id)
                .copied()
                .unwrap_or(false)
            {
                selected_text.map(TextInteractionOutcome::ToggleIngredient)
            } else {
                selected_text.map(TextInteractionOutcome::ToggleFavorite)
            }
        }
    };

    let Some(outcome) = outcome else {
        request_repaint(hwnd);
        return false;
    };

    match outcome {
        TextInteractionOutcome::ToggleFavorite(value) => {
            if favorites::toggle_snippet(&value).is_err() {
                request_repaint(hwnd);
                return false;
            }
            invalidate_favorites_cache();
        }
        TextInteractionOutcome::ToggleIngredient(value) => {
            if favorites::toggle_ingredient_snippet(&value).is_err() {
                request_repaint(hwnd);
                return false;
            }
            invalidate_favorites_cache();
        }
        TextInteractionOutcome::ToggleRecipe => {}
    }

    request_repaint(hwnd);
    true
}

pub(super) fn cancel_text_selection(hwnd: HWND) {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    if state.drag.is_some() {
        state.drag = None;
        request_repaint(hwnd);
    }
}

pub(super) fn text_selection_active(hwnd: HWND) -> bool {
    let state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    state
        .layout
        .as_ref()
        .is_some_and(|layout| layout.hwnd == hwnd)
        && state.drag.is_some()
}

fn selection_state() -> &'static Mutex<PopupSelectionState> {
    POPUP_SELECTION_STATE.get_or_init(|| Mutex::new(PopupSelectionState::default()))
}

pub(super) fn clear_selection_layout(hwnd: HWND) {
    if let Ok(mut state) = selection_state().lock() {
        if state
            .layout
            .as_ref()
            .is_some_and(|layout| layout.hwnd == hwnd)
        {
            state.layout = None;
        }
        state.drag = None;
        state.recipe_scroll_offset_px = 0;
    }
}

pub(super) fn clear_selection_state(hwnd: HWND) {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    if state
        .layout
        .as_ref()
        .is_some_and(|layout| layout.hwnd == hwnd)
    {
        state.layout = None;
    }
    state.drag = None;
    state.expanded_recipe_key = None;
    state.recipe_scroll_offset_px = 0;
}

pub(super) fn store_selection_layout(layout: SelectableLayout) {
    if let Ok(mut state) = selection_state().lock() {
        let max_scroll_offset = layout.recipe_scroll_max_offset_px.max(0);
        if let Some(ref existing_drag) = state.drag {
            let keep_drag = state
                .layout
                .as_ref()
                .is_some_and(|old| old.hwnd == layout.hwnd)
                && state
                    .layout
                    .as_ref()
                    .is_some_and(|old| old.items.get(existing_drag.item_id).is_some())
                && layout.items.get(existing_drag.item_id).is_some();
            if !keep_drag {
                state.drag = None;
            }
        }
        state.recipe_scroll_offset_px = state.recipe_scroll_offset_px.clamp(0, max_scroll_offset);
        state.layout = Some(layout);
    }
}

pub(in crate::popup) fn expanded_recipe_key() -> Option<RecipeExpansionKey> {
    selection_state().lock().ok()?.expanded_recipe_key
}

#[cfg(test)]
pub(in crate::popup) fn set_expanded_recipe_key_for_test(key: Option<RecipeExpansionKey>) {
    let mut state = selection_state().lock().expect("selection state lock");
    state.expanded_recipe_key = key;
    state.recipe_scroll_offset_px = 0;
}

pub(in crate::popup) fn recipe_detail_scroll_offset_px() -> i32 {
    selection_state()
        .lock()
        .ok()
        .map(|state| state.recipe_scroll_offset_px.max(0))
        .unwrap_or(0)
}

pub(super) fn scroll_recipe_detail_at(hwnd: HWND, x: i32, y: i32, delta: i32) -> bool {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let Some(layout) = state.layout.as_ref() else {
        return false;
    };
    if layout.hwnd != hwnd || layout.recipe_scroll_max_offset_px <= 0 {
        return false;
    }
    let Some(rect) = layout.recipe_scroll_rect else {
        return false;
    };
    if !point_in_rect(&rect, x, y) {
        return false;
    }

    let step = (layout.recipe_scroll_line_height * RECIPE_DETAIL_WHEEL_ROWS).max(1);
    let next = if delta > 0 {
        state.recipe_scroll_offset_px.saturating_sub(step)
    } else {
        state.recipe_scroll_offset_px.saturating_add(step)
    }
    .clamp(0, layout.recipe_scroll_max_offset_px);

    if next == state.recipe_scroll_offset_px {
        return true;
    }
    state.recipe_scroll_offset_px = next;
    request_repaint(hwnd);
    true
}

pub(super) fn collapse_recipe_detail_at(hwnd: HWND, x: i32, y: i32) -> bool {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let Some(layout) = state.layout.as_ref() else {
        return false;
    };
    if layout.hwnd != hwnd {
        return false;
    }
    let Some(rect) = layout.recipe_scroll_rect else {
        return false;
    };
    if !point_in_rect(&rect, x, y) || state.expanded_recipe_key.is_none() {
        return false;
    }

    state.expanded_recipe_key = None;
    state.recipe_scroll_offset_px = 0;
    request_repaint(hwnd);
    true
}

pub(super) fn content_cursor_kind_at(hwnd: HWND, x: i32, y: i32) -> Option<PopupCursorKind> {
    let state = selection_state().lock().ok()?;
    let layout = state.layout.as_ref()?;
    if layout.hwnd != hwnd {
        return None;
    }
    let (row, _) = hit_test_row(layout, x, y)?;
    if layout
        .item_recipe_keys
        .get(row.item_id)
        .copied()
        .flatten()
        .is_some()
        && !layout
            .item_ingredient_flags
            .get(row.item_id)
            .copied()
            .unwrap_or(false)
    {
        Some(PopupCursorKind::Hand)
    } else {
        Some(PopupCursorKind::Arrow)
    }
}

pub(super) fn current_selection_range(hwnd: HWND) -> Option<SelectionRange> {
    let state = selection_state().lock().ok()?;
    let layout = state.layout.as_ref()?;
    if layout.hwnd != hwnd {
        return None;
    }
    let drag = state.drag.as_ref()?;
    let item = layout.items.get(drag.item_id)?;
    let (mut start, mut end) = selected_range(drag.anchor, drag.current);
    start = start.min(item.len());
    end = end.min(item.len());
    if start >= end {
        return None;
    }
    Some(SelectionRange {
        item_id: drag.item_id,
        start,
        end,
    })
}

enum TextInteractionOutcome {
    ToggleFavorite(String),
    ToggleIngredient(String),
    ToggleRecipe,
}

fn hit_test_row(layout: &SelectableLayout, x: i32, y: i32) -> Option<(&SelectableRow, usize)> {
    let row = layout
        .rows
        .iter()
        .find(|row| point_in_selectable_row(row, x, y))?;
    let local = row_byte_index_from_x(row, x);
    Some((row, row.start + local))
}

fn hit_test_row_for_item(
    layout: &SelectableLayout,
    item_id: usize,
    x: i32,
    y: i32,
) -> Option<(&SelectableRow, usize)> {
    let mut closest = None;
    for row in layout.rows.iter().filter(|row| row.item_id == item_id) {
        if y >= row.top && y <= row.bottom {
            let local = row_byte_index_from_x(row, x);
            return Some((row, row.start + local));
        }
        let distance = if y < row.top {
            row.top - y
        } else {
            y - row.bottom
        };
        if closest.is_none_or(|(_, closest_distance)| distance < closest_distance) {
            closest = Some((row, distance));
        }
    }
    let (row, _) = closest?;
    let local = row_byte_index_from_x(row, x);
    Some((row, row.start + local))
}

fn row_byte_index_from_x(row: &SelectableRow, x: i32) -> usize {
    if row.boundaries.is_empty() {
        return 0;
    }
    let rel_x = (x - row.left).max(0);
    let mut previous = &row.boundaries[0];
    for boundary in row.boundaries.iter().skip(1) {
        let midpoint = previous.x_offset + (boundary.x_offset - previous.x_offset) / 2;
        if rel_x < midpoint {
            return previous.byte_index.min(row.end.saturating_sub(row.start));
        }
        previous = boundary;
    }
    previous.byte_index.min(row.end.saturating_sub(row.start))
}

fn point_in_selectable_row(row: &SelectableRow, x: i32, y: i32) -> bool {
    if y < row.top || y > row.bottom {
        return false;
    }
    let width = row
        .boundaries
        .last()
        .map(|boundary| boundary.x_offset)
        .unwrap_or(0)
        .max(1);
    x >= row.left && x <= row.left + width
}

fn selected_range(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}
