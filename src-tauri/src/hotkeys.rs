//! Global hotkeys (DESIGN §2.1). The plugin handler runs synchronously on the main thread
//! inside WM_HOTKEY: filter Pressed, send the vkE8 menu-mask key if Alt is down (and Ctrl is
//! not), then `std::thread::spawn` the action. Never block in the handler.

use crate::types::*;
use std::collections::BTreeMap;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Wry};

/// tauri-plugin-global-shortcut with our handler (dispatches to `actions::on_hotkey`, and Esc
/// to `actions::on_escape`).
pub fn plugin() -> TauriPlugin<Wry> {
    todo!()
}

/// (Re-)register hotkeys from settings. If `triggers` is false only `toggle` is registered
/// (glasses off). A combo that fails keeps the previously registered combo for that action.
/// Returns per-action error text, e.g. "Alt+Q is in use".
pub fn register_all(app: &AppHandle, s: &Settings, triggers: bool) -> BTreeMap<HotkeyAction, String> {
    todo!()
}

/// Register/unregister Esc (only during SELECT).
pub fn set_escape(app: &AppHandle, on: bool) {
    todo!()
}
