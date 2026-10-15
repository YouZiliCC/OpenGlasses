//! Tauri commands (DESIGN §3.8). Thin: validate, then delegate to the owning module.
//! All commands are `async` so they never run inline on the main thread.

use crate::types::*;
use crate::{actions, ai, capture, settings, state, themes, windows};
use tauri::{AppHandle, WebviewWindow};

#[tauri::command]
pub async fn frontend_ready(app: AppHandle, window: WebviewWindow) -> InitState {
    windows::frontend_ready(&app, window.label())
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Settings {
    state::settings(&app)
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<SaveSettingsResult, String> {
    actions::apply_settings(&app, settings)
}

#[tauri::command]
pub async fn set_api_key(id: String, key: String) -> Result<(), String> {
    settings::set_api_key(&id, &key)
}

#[tauri::command]
pub async fn has_api_key(id: String) -> bool {
    settings::has_api_key(&id)
}

#[tauri::command]
pub async fn delete_api_key(id: String) -> Result<(), String> {
    settings::delete_api_key(&id)
}

#[tauri::command]
pub async fn test_provider(app: AppHandle, provider: Provider) -> Result<String, String> {
    ai::test_provider(&app, provider).await
}

#[tauri::command]
pub async fn selection_done(
    app: AppHandle,
    window: WebviewWindow,
    rect: Option<Rect>,
    view_w: f64,
    view_h: f64,
) {
    capture::finish_select(&app, window.label(), rect.map(|r| (r, view_w, view_h)));
}

#[tauri::command]
pub async fn show_panel(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
    r#box: Rect,
    grow_up: bool,
    item_key: Option<String>,
) {
    windows::request_panel(&app, window.label(), &id, r#box, grow_up, item_key);
}

#[tauri::command]
pub async fn panel_set_height(app: AppHandle, id: String, css_h: f64) {
    windows::panel_set_height(&app, &id, css_h);
}

#[tauri::command]
pub async fn ask_followup(app: AppHandle, id: String, text: String) -> Result<(), String> {
    ai::followup(&app, &id, text)
}

#[tauri::command]
pub async fn dismiss_session(app: AppHandle, id: String) {
    ai::dismiss(&app, &id);
}

#[tauri::command]
pub async fn pin_session(app: AppHandle, id: String, pinned: bool) {
    ai::pin(&app, &id, pinned);
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    windows::open_settings(app).await;
}

#[tauri::command]
pub async fn get_history(app: AppHandle) -> Vec<HistoryEntry> {
    ai::history(&app)
}

#[tauri::command]
pub async fn list_themes(app: AppHandle) -> ThemesEvent {
    themes::list(&app)
}

#[tauri::command]
pub async fn reload_themes(app: AppHandle) -> ThemesEvent {
    themes::reload(&app)
}

#[tauri::command]
pub async fn open_themes_folder(app: AppHandle) -> Result<(), String> {
    themes::open_folder(&app)
}
