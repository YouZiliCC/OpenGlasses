//! Global hotkeys (DESIGN §2.1). The plugin handler runs synchronously on the main thread
//! inside WM_HOTKEY: filter Pressed, send the vkE8 menu-mask key if Alt is down (and Ctrl is
//! not), then `std::thread::spawn` the action. Never block in the handler.

use crate::actions;
use crate::types::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

/// Combos we currently hold, per action. The handler reads it on the main thread, and plugin
/// (un)register calls block on the main thread, so never hold this lock across a plugin call.
static REGISTERED: Mutex<BTreeMap<HotkeyAction, Shortcut>> = Mutex::new(BTreeMap::new());
/// Serializes `register_all` callers.
static REGISTERING: Mutex<()> = Mutex::new(());

const ACTIONS: [HotkeyAction; 4] = [
    HotkeyAction::Explain,
    HotkeyAction::Capture,
    HotkeyAction::Scan,
    HotkeyAction::Toggle,
];

pub(crate) const INVALID: &str = "invalid hotkey";

fn escape() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

/// tauri-plugin-global-shortcut with our handler (dispatches to `actions::on_hotkey`, and Esc
/// to `actions::on_escape`).
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::<Wry>::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            mask_menu_key();
            let id = shortcut.id();
            let app = app.clone();
            if id == escape().id() {
                std::thread::spawn(move || actions::on_escape(&app));
            } else if let Some(action) = action_of(id) {
                std::thread::spawn(move || actions::on_hotkey(&app, action));
            }
        })
        .build()
}

fn action_of(id: u32) -> Option<HotkeyAction> {
    REGISTERED
        .lock()
        .unwrap()
        .iter()
        .find(|(_, s)| s.id() == id)
        .map(|(a, _)| *a)
}

/// With Alt held, releasing it would activate the target's menu bar; an unassigned key event
/// in between (vkE8) suppresses that. Skipped with Ctrl down (Ctrl+Alt never opens the menu).
fn mask_menu_key() {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    unsafe {
        if GetAsyncKeyState(VK_MENU.0 as i32) >= 0 || GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 {
            return;
        }
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0xE8),
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        SendInput(
            &[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)],
            size_of::<INPUT>() as i32,
        );
    }
}

/// (Re-)register hotkeys from settings. If `triggers` is false only `toggle` is registered
/// (glasses off). A combo that fails keeps the previously registered combo for that action.
/// Returns per-action error text, e.g. "Alt+Q is in use".
pub fn register_all(
    app: &AppHandle,
    s: &Settings,
    triggers: bool,
) -> BTreeMap<HotkeyAction, String> {
    let _serial = REGISTERING.lock().unwrap_or_else(|e| e.into_inner());
    let gs = app.global_shortcut();
    let old = REGISTERED.lock().unwrap().clone();
    let mut errors = BTreeMap::new();

    // action -> (combo, text as typed)
    let mut want = BTreeMap::new();
    for a in ACTIONS {
        if !triggers && a != HotkeyAction::Toggle {
            continue;
        }
        let Some(text) = s.hotkeys.get(&a) else {
            continue;
        };
        match text.parse::<Shortcut>() {
            Ok(sc) => {
                want.insert(a, (sc, text.clone()));
            }
            Err(_) => {
                errors.insert(a, INVALID.to_string());
                if let Some(o) = old.get(&a) {
                    want.insert(a, (*o, text.clone()));
                }
            }
        }
    }

    // Release changed combos first so swapped combos can be taken by the other action.
    for (a, sc) in &old {
        if want.get(a).map(|w| w.0) != Some(*sc) {
            let _ = gs.unregister(*sc);
        }
    }

    let mut now = BTreeMap::new();
    for (a, (sc, text)) in want {
        if old.get(&a) == Some(&sc) || gs.register(sc).is_ok() {
            now.insert(a, sc);
            continue;
        }
        errors.insert(a, format!("{text} is in use"));
        if let Some(o) = old.get(&a) {
            if gs.register(*o).is_ok() {
                now.insert(a, *o);
            }
        }
    }

    *REGISTERED.lock().unwrap() = now;
    errors
}

/// Register/unregister Esc (only during SELECT).
pub fn set_escape(app: &AppHandle, on: bool) {
    let gs = app.global_shortcut();
    let esc = escape();
    if gs.is_registered(esc) == on {
        return;
    }
    let r = if on {
        gs.register(esc)
    } else {
        gs.unregister(esc)
    };
    if let Err(e) = r {
        log::warn!("Esc hotkey: {e}");
    }
}
