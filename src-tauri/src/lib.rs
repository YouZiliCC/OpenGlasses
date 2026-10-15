//! OpenGlasses core. Module map: docs/DESIGN.md §4.

mod actions;
mod ai;
mod capture;
mod commands;
mod cursor;
mod grab;
mod hotkeys;
mod settings;
mod state;
mod telemetry;
mod themes;
mod tray;
mod types;
mod windows;

use state::AppState;
use tauri::{Manager, RunEvent};

pub fn run() {
    let app = tauri::Builder::default()
        // Must be the first plugin: a second launch only opens the settings window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tauri::async_runtime::spawn(windows::open_settings(app.clone()));
        }))
        .plugin(hotkeys::plugin())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("ogframe", capture::serve)
        .register_asynchronous_uri_scheme_protocol("ogtheme", themes::serve)
        .invoke_handler(tauri::generate_handler![
            commands::frontend_ready,
            commands::get_settings,
            commands::save_settings,
            commands::set_api_key,
            commands::has_api_key,
            commands::delete_api_key,
            commands::test_provider,
            commands::selection_done,
            commands::show_panel,
            commands::panel_set_height,
            commands::ask_followup,
            commands::dismiss_session,
            commands::pin_session,
            commands::open_settings,
            commands::get_history,
            commands::list_themes,
            commands::reload_themes,
            commands::open_themes_folder,
        ])
        .on_window_event(|w, e| {
            // Overlays live for the whole run: Alt+F4 in the (focused) panel only closes the
            // answer, like Esc; lenses are never focused but must not be closable either.
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                if w.label() != "settings" {
                    api.prevent_close();
                    if w.label() == "panel" {
                        let app = w.app_handle().clone();
                        std::thread::spawn(move || ai::dismiss_active(&app));
                    }
                }
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = settings::load(&handle);
            app.manage(AppState::new(settings));
            windows::setup(&handle)?;
            tray::setup(&handle)?;
            telemetry::spawn(handle.clone());
            cursor::spawn(handle.clone());
            themes::spawn_watcher(handle.clone());
            actions::startup(&handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building OpenGlasses");

    app.run(|_app, event| {
        // A tray app: closing the settings window must not quit.
        if let RunEvent::ExitRequested { api, code: None, .. } = event {
            api.prevent_exit();
        }
    });
}
