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

pub(super) fn update_hovered_header_button(hwnd: HWND, action: Option<HeaderButtonAction>) -> bool {
    let store = POPUP_HEADER_HOVER.get_or_init(|| Mutex::new(None));
    let mut guard = match store.lock() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let next = action.map(|action| HeaderButtonHover { hwnd, action });
    if guard.as_ref().map(|hover| (hover.hwnd, hover.action))
        == next.as_ref().map(|hover| (hover.hwnd, hover.action))
    {
        return false;
    }
    *guard = next;
    true
}

pub(super) fn hovered_header_button(hwnd: HWND) -> Option<HeaderButtonAction> {
    let store = POPUP_HEADER_HOVER.get_or_init(|| Mutex::new(None));
    let guard = store.lock().ok()?;
    guard
        .as_ref()
        .filter(|hover| hover.hwnd == hwnd)
        .map(|hover| hover.action)
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
            lines: Arc::new(build_lines(state)),
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
            interrupted,
            turbulence,
        } => Some(PopupAnimationFrame::Switch {
            old_lines: old_lines.clone(),
            new_lines: new_lines.clone(),
            old_title: old_title.clone(),
            new_title: new_title.clone(),
            direction: *direction,
            progress,
            interrupted: *interrupted,
            turbulence: *turbulence,
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
            lines: Arc::new(build_lines(state)),
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
    let stacked = stacked_switch_count(hwnd);
    let interrupted = stacked > 0;
    start_animation(
        hwnd,
        if interrupted {
            POPUP_INTERRUPTED_SWITCH_ANIM_MS
        } else {
            POPUP_SWITCH_ANIM_MS
        },
        PopupAnimationKind::Switch {
            old_lines: Arc::new(build_lines(old_state)),
            new_lines: Arc::new(build_lines(new_state)),
            old_title: header_title(old_state),
            new_title: header_title(new_state),
            direction,
            interrupted,
            turbulence: turbulence_for_stack(stacked),
        },
    );
}

/// How much header dither this switch earns, from how many switches have landed
/// on top of one another without one being allowed to finish.
///
/// Deliberately the opposite of how the body reads. A single deliberate switch
/// resolves clean and instant, so the name is legible the moment you arrive.
/// Spinning the wheel puts the title into visible churn instead, which is honest
/// about the state: the names are going past faster than they can be read, and
/// the stipple says so rather than flashing a sequence of half-legible words.
fn turbulence_for_stack(stacked: u32) -> f32 {
    (stacked as f32 / POPUP_SWITCH_TURBULENCE_SATURATION).clamp(0.0, 1.0)
}

/// Switches already stacked up on the running animation, saturating so a long
/// spin cannot run the counter away and leave the title stippled for ages after
/// the user stops.
fn stacked_switch_count(hwnd: HWND) -> u32 {
    let store = POPUP_ANIMATION.get_or_init(|| Mutex::new(None));
    let Ok(guard) = store.lock() else {
        return 0;
    };
    match guard.as_ref() {
        Some(anim) if anim.hwnd == hwnd => match &anim.kind {
            PopupAnimationKind::Switch { turbulence, .. } => {
                let previous = (turbulence * POPUP_SWITCH_TURBULENCE_SATURATION).round() as u32;
                (previous + 1).min(POPUP_SWITCH_TURBULENCE_SATURATION as u32)
            }
            _ => 0,
        },
        _ => 0,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A single deliberate switch has to land perfectly clean. This is the whole
    /// point of driving the dither from velocity rather than from progress: the
    /// common case, clicking an arrow, must look like a direct switch.
    #[test]
    fn a_lone_switch_earns_no_turbulence() {
        assert_eq!(turbulence_for_stack(0), 0.0);
    }

    #[test]
    fn stacked_switches_ramp_turbulence_to_full() {
        assert!(turbulence_for_stack(1) > 0.0);
        assert!(turbulence_for_stack(1) < turbulence_for_stack(2));
        assert_eq!(turbulence_for_stack(4), 1.0);
    }

    /// Saturating matters as much as ramping. A long spin must not bank
    /// turbulence it would spend stippling the title long after the user stopped.
    #[test]
    fn turbulence_saturates_rather_than_running_away() {
        assert_eq!(turbulence_for_stack(50), 1.0);
    }
}
