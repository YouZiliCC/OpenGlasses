//! Process-wide state. Each module owns its own sub-state struct; other modules go
//! through that module's functions instead of touching its fields.
//! Never hold one of these locks across `.await` or an `emit` (DESIGN §3.1 threading rules).

use crate::types::{HudState, Settings};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, RwLock};
use tauri::{AppHandle, Manager};

pub struct AppState {
    /// Owner: settings.rs (writes go through `settings::save`).
    pub settings: RwLock<Settings>,
    /// Owner: telemetry.rs (writes go through `telemetry::hud_update`).
    pub hud: Mutex<HudState>,
    /// Owner: actions.rs (`actions::set_glasses`).
    pub glasses_on: AtomicBool,
    pub win: crate::windows::WinState,
    pub ai: crate::ai::AiState,
    pub capture: crate::capture::CaptureState,
    pub flow: crate::actions::FlowState,
    pub themes: crate::themes::ThemeState,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        AppState {
            settings: RwLock::new(settings),
            hud: Mutex::new(HudState::default()),
            glasses_on: AtomicBool::new(true),
            win: Default::default(),
            ai: Default::default(),
            capture: Default::default(),
            flow: Default::default(),
            themes: Default::default(),
        }
    }
}

/// Shorthand: `st(app).settings.read()`.
pub fn st(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

/// Clone of the current settings (cheap enough; avoids holding the lock).
pub fn settings(app: &AppHandle) -> Settings {
    st(app).settings.read().unwrap().clone()
}
