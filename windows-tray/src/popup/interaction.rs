use super::content::invalidate_favorites_cache;
use super::layout::{header_layout, popup_scale};
use super::*;

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
        let scale = popup_scale(settings);
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
    });
    request_repaint(hwnd);
    true
}

pub(super) fn update_text_selection(hwnd: HWND, x: i32, y: i32) {
    let mut state = match selection_state().lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    let layout = match state.layout.as_ref() {
        Some(value) if value.hwnd == hwnd => value.clone(),
        _ => return,
    };
    let Some(drag) = state.drag.as_mut() else {
        return;
    };
    let Some((row, next_index)) = hit_test_row_for_item(&layout, drag.item_id, x, y) else {
        return;
    };
    if row.item_id != drag.item_id {
        return;
    }
    if drag.current != next_index {
        drag.current = next_index;
        request_repaint(hwnd);
    }
}

pub(super) fn finish_text_selection(hwnd: HWND, x: i32, y: i32) -> bool {
    let snippet = {
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
        if selected.0 == selected.1 {
            None
        } else {
            let item = layout.items.get(drag.item_id);
            item.and_then(|text| {
                text.get(selected.0..selected.1)
                    .map(|value| favorites::normalize_snippet(value))
                    .filter(|value| !value.is_empty())
            })
        }
    };

    let Some(value) = snippet else {
        request_repaint(hwnd);
        return false;
    };

    if favorites::toggle_snippet(&value).is_err() {
        request_repaint(hwnd);
        return false;
    }

    invalidate_favorites_cache();
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
    }
}

pub(super) fn clear_selection_state(hwnd: HWND) {
    clear_selection_layout(hwnd);
}

pub(super) fn store_selection_layout(layout: SelectableLayout) {
    if let Ok(mut state) = selection_state().lock() {
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
        state.layout = Some(layout);
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

fn hit_test_row(layout: &SelectableLayout, x: i32, y: i32) -> Option<(&SelectableRow, usize)> {
    let row = layout
        .rows
        .iter()
        .find(|row| y >= row.top && y <= row.bottom)?;
    let local = row_byte_index_from_x(row, x);
    Some((row, row.start + local))
}

fn hit_test_row_for_item(
    layout: &SelectableLayout,
    item_id: usize,
    x: i32,
    y: i32,
) -> Option<(&SelectableRow, usize)> {
    let item_rows: Vec<&SelectableRow> = layout
        .rows
        .iter()
        .filter(|row| row.item_id == item_id)
        .collect();
    if item_rows.is_empty() {
        return None;
    }

    let row = item_rows
        .iter()
        .copied()
        .find(|row| y >= row.top && y <= row.bottom)
        .or_else(|| {
            item_rows.iter().copied().min_by_key(|row| {
                if y < row.top {
                    row.top - y
                } else if y > row.bottom {
                    y - row.bottom
                } else {
                    0
                }
            })
        })?;
    let local = row_byte_index_from_x(row, x);
    Some((row, row.start + local))
}

fn row_byte_index_from_x(row: &SelectableRow, x: i32) -> usize {
    if row.boundaries.is_empty() {
        return 0;
    }
    let rel_x = (x - row.left).max(0);
    let mut selected = 0usize;
    for boundary in &row.boundaries {
        if boundary.x_offset <= rel_x {
            selected = boundary.byte_index;
        } else {
            break;
        }
    }
    selected.min(row.end.saturating_sub(row.start))
}

fn selected_range(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}
