//! 1 Hz telemetry loop + HUD state (DESIGN §2.5, §3.6).
//! Each tick while glasses are on: sysinfo CPU/RAM, NVML GPU (init once; skipped on battery
//! systems), foreground process name, fullscreen-foreground suppression of lenses, then
//! emit `og://hud` to the primary lens.

use crate::types::*;
use tauri::AppHandle;

pub fn spawn(app: AppHandle) {
    todo!()
}

/// Mutate the HUD state and emit it to the primary lens. The only writer of `og://hud`.
pub fn hud_update(app: &AppHandle, f: impl FnOnce(&mut HudState)) {
    todo!()
}

pub fn set_mode(app: &AppHandle, mode: HudMode) {
    todo!()
}

/// After a finished answer: add tokens to usage.json (date-keyed) and update ttft/tps/tokensToday.
pub fn record_usage(app: &AppHandle, output_tokens: u64, ttft_ms: Option<u64>, tps: Option<f64>) {
    todo!()
}
