//! The single 60 Hz polling loop (no hooks). DESIGN §2.2 rules 6–7, §3.4 SELECT end.
//!
//! Each tick:
//! - GetCursorPos; find the lens under the cursor; emit `og://cursor` `{x,y,inPanel}` (lens CSS
//!   px) to that lens only when position/inPanel changed; emit `null` to a lens the cursor left.
//! - Left button (or right if SM_SWAPBUTTON) up→down edge, not selecting, outside the panel
//!   rect → `actions::on_click_outside`.
//! - While selecting: left released outside the frozen lens's monitor → cancel
//!   (`capture::finish_select(app, label, None)`); right button down → cancel.
//! - Foreground watch for the active session's source window: foreground is neither the
//!   source hwnd nor ours, or the source was minimized / moved / resized
//!   → `actions::on_source_changed`.

use crate::types::{ev, CursorPayload};
use crate::{actions, ai, capture, grab, state, windows};
use ::windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VIRTUAL_KEY, VK_LBUTTON, VK_RBUTTON};
use ::windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::AppHandle;

const TICK: Duration = Duration::from_millis(16);

pub fn spawn(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("og-cursor".into()).spawn(move || {
        let mut t = Tracker::default();
        loop {
            std::thread::sleep(TICK);
            t.tick(&app);
        }
    });
    if let Err(e) = spawned {
        log::error!("cursor thread: {e}");
    }
}

#[derive(Default)]
struct Tracker {
    /// Last `og://cursor` sent: (lens label, physical x, y, inPanel).
    last: Option<(String, i32, i32, bool)>,
    primary: bool,
    secondary: bool,
    /// Session whose source change was already reported (fire once per session).
    reported: Option<String>,
}

impl Tracker {
    fn tick(&mut self, app: &AppHandle) {
        let (primary, secondary) = buttons();
        let pressed = primary && !self.primary;
        let released = !primary && self.primary;
        let secondary_pressed = secondary && !self.secondary;
        self.primary = primary;
        self.secondary = secondary;

        if !state::st(app).glasses_on.load(Ordering::SeqCst) {
            if let Some((label, ..)) = self.last.take() {
                windows::emit_to_window(app, &label, ev::CURSOR, None::<CursorPayload>);
            }
            return;
        }
        let Some((x, y)) = windows::try_cursor_pos() else { return };
        let in_panel = windows::panel_rect(app).is_some_and(|r| r.contains(x, y));
        self.emit_cursor(app, x, y, in_panel);

        if let Some(sel) = capture::selecting_lens(app) {
            let outside = || windows::lens_by_label(app, &sel).is_none_or(|l| !l.bounds.contains(x, y));
            if secondary_pressed || (released && outside()) {
                capture::finish_select(app, &sel, None);
            }
            return;
        }
        if pressed && !in_panel {
            actions::on_click_outside(app);
        }
        self.watch_source(app);
    }

    fn emit_cursor(&mut self, app: &AppHandle, x: i32, y: i32, in_panel: bool) {
        let Some(lens) = windows::lens_at(app, x, y).filter(|l| l.ready) else { return };
        let cur = (lens.label.clone(), x, y, in_panel);
        if self.last.as_ref() == Some(&cur) {
            return;
        }
        if let Some((prev, ..)) = &self.last {
            if *prev != lens.label {
                windows::emit_to_window(app, prev, ev::CURSOR, None::<CursorPayload>);
            }
        }
        let p = lens.to_css_point(x, y);
        let payload = Some(CursorPayload { x: p.x, y: p.y, in_panel });
        windows::emit_to_window(app, &lens.label, ev::CURSOR, payload);
        self.last = Some(cur);
    }

    fn watch_source(&mut self, app: &AppHandle) {
        let Some(meta) = ai::active_meta(app) else {
            self.reported = None;
            return;
        };
        let Some(src) = meta.source.as_ref().filter(|s| s.hwnd != 0) else { return };
        if self.reported.as_deref() == Some(meta.id.as_str()) {
            return;
        }
        let fg = grab::foreground_hwnd();
        // fg is briefly 0 while activation moves between windows: not a change by itself.
        let lost_foreground = fg != 0 && fg != src.hwnd && !windows::is_own_window(fg);
        let changed = lost_foreground
            || grab::window_state(src.hwnd).is_none_or(|(rect, minimized)| minimized || rect != src.rect);
        if changed {
            self.reported = Some(meta.id.clone());
            actions::on_source_changed(app);
        }
    }
}

/// (primary down, secondary down). GetAsyncKeyState reads the physical buttons, so with
/// SM_SWAPBUTTON the primary (logical left) button is VK_RBUTTON.
fn buttons() -> (bool, bool) {
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let (p, s) = if swapped { (VK_RBUTTON, VK_LBUTTON) } else { (VK_LBUTTON, VK_RBUTTON) };
    let down = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
    (down(p), down(s))
}
