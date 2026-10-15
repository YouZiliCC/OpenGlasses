//! Tray icon: "Glasses On/Off", "Settings…", "Quit" (DESIGN §2.2 rule 10).
//! Menu callbacks run on the main thread: only dispatch (spawn), never create windows inline.

use crate::state::st;
use crate::{actions, windows};
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

/// Handle to the "Glasses: On/Off" item.
struct GlassesItem(MenuItem<Wry>);

fn glasses_text(on: bool) -> &'static str {
    if on {
        "Glasses: On"
    } else {
        "Glasses: Off"
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let on = st(app).glasses_on.load(Ordering::SeqCst);
    let toggle = MenuItem::with_id(app, "toggle", glasses_text(on), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("OpenGlasses")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let on = st(&app).glasses_on.load(Ordering::SeqCst);
                    actions::set_glasses(&app, !on);
                });
            }
            "settings" => {
                tauri::async_runtime::spawn(windows::open_settings(app.clone()));
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(GlassesItem(toggle));
    Ok(())
}

/// Update the "Glasses On/Off" item text/check state.
pub fn set_glasses_item(app: &AppHandle, on: bool) {
    if let Some(item) = app.try_state::<GlassesItem>() {
        if let Err(e) = item.0.set_text(glasses_text(on)) {
            log::warn!("tray: {e}");
        }
    }
}
