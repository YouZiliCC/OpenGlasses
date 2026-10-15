//! Selected-text grabbing (DESIGN §3.3): UIA first, clipboard fallback second.
//! Everything here is blocking and must run on a worker thread.

use crate::types::*;

pub enum GrabResult {
    /// `anchor` = union of UIA selection rects (physical px), when available and ≤2000 chars.
    Text { text: String, anchor: Option<PhysRect>, source: SourceInfo },
    /// Definitely nothing selected (UIA TextPattern with an empty selection, or copy produced nothing).
    Empty { source: SourceInfo },
    /// Abort with a toast code: "RELEASE KEYS", "ELEVATED APP", "NO SELECTION".
    Toast(&'static str),
    /// Foreground window is ours (or none): do nothing.
    Ignore,
}

/// Foreground window info (hwnd, process exe name, title, DWM extended frame bounds,
/// minimized). None if there is no foreground window.
pub fn foreground_source() -> Option<SourceInfo> {
    todo!()
}

/// Current GetForegroundWindow as isize.
pub fn foreground_hwnd() -> isize {
    todo!()
}

/// Rect (DWM extended frame bounds) and minimized flag of a window; None if it no longer exists.
pub fn window_state(hwnd: isize) -> Option<(PhysRect, bool)> {
    todo!()
}

/// Full grab pipeline. Steps 1–4 of DESIGN §3.3, including the elevation check,
/// the terminal Ctrl+Insert rule, modifier wait, clipboard snapshot/restore.
pub fn grab_selection() -> GrabResult {
    todo!()
}

/// Window class name of a window (used for terminal / shell detection).
pub fn class_name(hwnd: isize) -> String {
    todo!()
}
