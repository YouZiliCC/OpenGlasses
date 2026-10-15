//! settings.json (serde_json, temp file + rename), usage.json, presets, and API keys in the
//! Windows Credential Manager via `keyring` (service "openglasses", user = slot id).
//! DESIGN §3.5 (presets/extraBody) and §3.7.

use crate::types::*;
use tauri::AppHandle;

pub const KEYRING_SERVICE: &str = "openglasses";

/// The 8 fixed provider slots + all other defaults.
pub fn defaults() -> Settings {
    todo!()
}

/// Load from the app config dir; missing/corrupt file -> defaults. Always returns exactly the
/// 8 slots (fill missing slots from defaults, drop unknown ids).
pub fn load(app: &AppHandle) -> Settings {
    todo!()
}

/// Persist atomically and update `AppState.settings`. Does not emit or re-register anything.
pub fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    todo!()
}

pub fn active_provider(s: &Settings) -> Provider {
    todo!()
}

/// True if a request can be attempted: key present, or baseUrl is localhost/127.0.0.1.
pub fn provider_usable(p: &Provider) -> bool {
    todo!()
}

pub fn get_api_key(id: &str) -> Option<String> {
    todo!()
}

pub fn set_api_key(id: &str, key: &str) -> Result<(), String> {
    todo!()
}

pub fn has_api_key(id: &str) -> bool {
    todo!()
}

pub fn delete_api_key(id: &str) -> Result<(), String> {
    todo!()
}

/// Add tokens to today's counter in usage.json; returns today's total.
pub fn add_usage(app: &AppHandle, tokens: u64) -> u64 {
    todo!()
}

/// Today's total from usage.json (0 if another day).
pub fn usage_today(app: &AppHandle) -> u64 {
    todo!()
}
