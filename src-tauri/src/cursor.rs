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

use tauri::AppHandle;

pub fn spawn(app: AppHandle) {
    todo!()
}
