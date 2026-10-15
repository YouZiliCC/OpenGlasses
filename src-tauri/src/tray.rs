//! Tray icon: "Glasses On/Off", "Settings…", "Quit" (DESIGN §2.2 rule 10).
//! Menu callbacks run on the main thread: only dispatch (spawn), never create windows inline.

use tauri::AppHandle;

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    todo!()
}

/// Update the "Glasses On/Off" item text/check state.
pub fn set_glasses_item(app: &AppHandle, on: bool) {
    todo!()
}
