//! Runtime theme packs (DESIGN §10): discovery in the built-in and user theme folders, the
//! `ogtheme://` protocol, and 1 s fingerprint polling for hot reload. CSS + static assets only.

use crate::types::*;
use std::sync::Mutex;
use tauri::http::Request;
use tauri::{AppHandle, UriSchemeContext, UriSchemeResponder, Wry};

#[derive(Default)]
pub struct ThemeState {
    /// Last scan result + revision (bumped on every change).
    pub current: Mutex<Option<ThemesEvent>>,
}

/// Built-in themes: `<resource_dir>/themes`. User themes: `<app_config_dir>/themes` (created
/// on demand). A user theme overrides a built-in one with the same id.
pub fn user_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    todo!()
}

/// Current themes (scans on first use).
pub fn list(app: &AppHandle) -> ThemesEvent {
    todo!()
}

/// Force a rescan; bump `rev` and broadcast `og://themes` if anything changed.
pub fn reload(app: &AppHandle) -> ThemesEvent {
    todo!()
}

/// Poll both theme folders every second (path, size, mtime fingerprint); on change -> `reload`.
pub fn spawn_watcher(app: AppHandle) {
    todo!()
}

/// `ogtheme://<id>/<relative path>`: serve a file from that theme's folder only (no `..`,
/// no backslashes, extension whitelist, ≤ 8 MB) with Content-Type, `Cache-Control: no-store`
/// and `Access-Control-Allow-Origin: *` (mask-image / @font-face fetch in CORS mode); else 404.
pub fn serve(ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    todo!()
}

/// Open the user theme folder in Explorer (create it first).
pub fn open_folder(app: &AppHandle) -> Result<(), String> {
    todo!()
}
