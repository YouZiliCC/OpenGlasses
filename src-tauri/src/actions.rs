//! Orchestration: hotkey actions and the session rules of DESIGN §2.2. All entry points run on
//! worker threads (spawned by hotkeys.rs / cursor.rs / tray.rs), never on the main thread.

use crate::capture::Shot;
use crate::types::*;
use std::sync::Mutex;
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Grabbing,
    Selecting,
    Scanning,
}

#[derive(Default)]
pub struct FlowState {
    pub phase: Mutex<Phase>,
}

/// After setup: register hotkeys (toast conflicts), wait for the primary lens to be ready then
/// play Lens-On (if enabled), and on first run with no usable provider open Settings.
pub fn startup(app: &AppHandle) {
    todo!()
}

/// Dispatch one hotkey action according to §2.2 rules 2–4 (and ignore while Grabbing).
pub fn on_hotkey(app: &AppHandle, action: HotkeyAction) {
    todo!()
}

/// Esc pressed during SELECT -> cancel.
pub fn on_escape(app: &AppHandle) {
    todo!()
}

/// Show a short warning next to the cursor (`og://toast` to the cursor's lens).
pub fn toast(app: &AppHandle, text: &str) {
    todo!()
}

/// From cursor.rs: left-button down outside the panel (rule 6).
pub fn on_click_outside(app: &AppHandle) {
    todo!()
}

/// From cursor.rs: the active session's source lost foreground / moved / minimized (rule 7).
pub fn on_source_changed(app: &AppHandle) {
    todo!()
}

/// From capture.rs: the user finished a region selection. `rect` is physical px, `shot` the frozen frame.
pub fn on_selection(app: &AppHandle, lens: LensInfo, shot: Shot, rect: PhysRect) {
    todo!()
}

/// Glasses on/off (rule 10). Also used by the tray.
pub fn set_glasses(app: &AppHandle, on: bool) {
    todo!()
}

/// `save_settings`: persist, re-register hotkeys, apply content protection + glass mode,
/// broadcast `og://settings`; returns hotkey errors.
pub fn apply_settings(app: &AppHandle, s: Settings) -> Result<SaveSettingsResult, String> {
    todo!()
}
