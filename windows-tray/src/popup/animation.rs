use super::content::build_lines;
use super::interaction::clear_selection_state;
use super::layout::{header_title, hide_popup};
use super::*;

pub(super) fn press_navigation_button(hwnd: HWND, direction: i32) {
    let action = if direction < 0 {
        HeaderButtonAction::Prev
    } else if direction > 0 {
        HeaderButtonAction::Next
    } else {
        return;
    };

    let store = POPUP_HEADER_PRESS.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        *guard = Some(HeaderButtonPress {
            hwnd,
            action,
            until_epoch_ms: now_epoch_ms() + POPUP_HEADER_PRESS_MS,
        });
    }
    unsafe {
        let _ = SetTimer(
            hwnd,
            POPUP_HEADER_PRESS_TIMER_ID,
            POPUP_HEADER_PRESS_MS.max(1) as u32,
            None,
        );
        request_repaint(hwnd);
    }
}

pub(super) fn tick_header_button_press(hwnd: HWND) {
    let store = POPUP_HEADER_PRESS.get_or_init(|| Mutex::new(None));
    let should_clear = match store.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(press) => press.hwnd == hwnd && now_epoch_ms() >= press.until_epoch_ms,
            None => true,
        },
        Err(_) => true,
    };
    if should_clear {
        clear_header_button_press(hwnd);
        unsafe {
            let _ = KillTimer(hwnd, POPUP_HEADER_PRESS_TIMER_ID);
            request_repaint(hwnd);
        }
    }
}

pub(super) fn clear_header_button_press(hwnd: HWND) {
    let store = POPUP_HEADER_PRESS.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        if guard.as_ref().is_some_and(|press| press.hwnd == hwnd) {
            *guard = None;
        }
    }
}

pub(super) fn pressed_header_button(hwnd: HWND) -> Option<HeaderButtonAction> {
    let store = POPUP_HEADER_PRESS.get_or_init(|| Mutex::new(None));
    let mut guard = store.lock().ok()?;
    let now = now_epoch_ms();
    if let Some(press) = guard.as_ref() {
        if press.hwnd != hwnd || now >= press.until_epoch_ms {
            *guard = None;
            return None;
        }
        return Some(press.action);
    }
    None
}

pub(super) fn begin_open_animation(hwnd: HWND, state: &AppState) {
    if !popup_animations_enabled(&state.settings) {
        clear_animation_state(hwnd);
        request_repaint(hwnd);
        return;
    }
    start_animation(
        hwnd,
        POPUP_OPEN_ANIM_MS,
        PopupAnimationKind::Open {
            lines: build_lines(state),
            title: header_title(state),
        },
    );
}

fn start_animation(hwnd: HWND, duration_ms: i64, kind: PopupAnimationKind) {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        *guard = Some(PopupAnimation {
            hwnd,
            start_epoch_ms: now_epoch_ms(),
            duration_ms: duration_ms.max(1),
            kind,
        });
    }
    unsafe {
        let _ = SetTimer(hwnd, POPUP_ANIM_TIMER_ID, POPUP_ANIM_INTERVAL_MS, None);
        request_repaint(hwnd);
    }
}

pub(super) fn clear_animation_state(hwnd: HWND) {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        if guard.as_ref().is_some_and(|anim| anim.hwnd == hwnd) {
            *guard = None;
        }
    }
}

fn close_animation_active(hwnd: HWND) -> bool {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    match store.lock() {
        Ok(guard) => guard.as_ref().is_some_and(|anim| {
            anim.hwnd == hwnd && matches!(anim.kind, PopupAnimationKind::Close { .. })
        }),
        Err(_) => false,
    }
}

pub(super) fn current_animation_frame(hwnd: HWND) -> Option<PopupAnimationFrame> {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    let guard = store.lock().ok()?;
    let anim = guard.as_ref()?;
    if anim.hwnd != hwnd {
        return None;
    }
    let elapsed = now_epoch_ms().saturating_sub(anim.start_epoch_ms);
    let progress = (elapsed as f32 / anim.duration_ms.max(1) as f32).clamp(0.0, 1.0);
    match &anim.kind {
        PopupAnimationKind::Open { lines, title } => Some(PopupAnimationFrame::Open {
            lines: lines.clone(),
            title: title.clone(),
            progress,
        }),
        PopupAnimationKind::Close { lines, title } => Some(PopupAnimationFrame::Close {
            lines: lines.clone(),
            title: title.clone(),
            progress,
        }),
        PopupAnimationKind::Switch {
            old_lines,
            new_lines,
            old_title,
            new_title,
            direction,
        } => Some(PopupAnimationFrame::Switch {
            old_lines: old_lines.clone(),
            new_lines: new_lines.clone(),
            old_title: old_title.clone(),
            new_title: new_title.clone(),
            direction: *direction,
            progress,
        }),
    }
}

pub(super) fn begin_close_animation(hwnd: HWND, state: &AppState) {
    if !is_visible(hwnd) {
        return;
    }
    if close_animation_active(hwnd) {
        return;
    }
    clear_selection_state(hwnd);
    if !popup_animations_enabled(&state.settings) {
        hide_popup(hwnd);
        return;
    }
    start_animation(
        hwnd,
        POPUP_CLOSE_ANIM_MS,
        PopupAnimationKind::Close {
            lines: build_lines(state),
            title: header_title(state),
        },
    );
}

pub(super) fn begin_switch_animation(
    hwnd: HWND,
    old_state: &AppState,
    new_state: &AppState,
    direction: i32,
) {
    clear_selection_state(hwnd);
    if !popup_animations_enabled(&new_state.settings) {
        clear_animation_state(hwnd);
        request_repaint(hwnd);
        return;
    }
    start_animation(
        hwnd,
        POPUP_SWITCH_ANIM_MS,
        PopupAnimationKind::Switch {
            old_lines: build_lines(old_state),
            new_lines: build_lines(new_state),
            old_title: header_title(old_state),
            new_title: header_title(new_state),
            direction,
        },
    );
}

pub(super) fn tick_animation(hwnd: HWND) {
    let now = now_epoch_ms();
    let mut active = false;
    let mut finished = false;
    let mut hide_after = false;

    {
        let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
        let mut guard = match store.lock() {
            Ok(value) => value,
            Err(_) => return,
        };
        if let Some(anim) = guard.as_ref() {
            if anim.hwnd == hwnd {
                active = true;
                let elapsed = now.saturating_sub(anim.start_epoch_ms);
                if elapsed >= anim.duration_ms.max(1) {
                    finished = true;
                    hide_after = matches!(anim.kind, PopupAnimationKind::Close { .. });
                }
            }
        }
        if finished {
            *guard = None;
        }
    }

    unsafe {
        if !active {
            let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
            return;
        }
        if finished {
            let _ = KillTimer(hwnd, POPUP_ANIM_TIMER_ID);
            if hide_after {
                ShowWindow(hwnd, SW_HIDE);
                return;
            }
        }
        request_repaint(hwnd);
    }
}
