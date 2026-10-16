//! 1 Hz telemetry loop + HUD state (DESIGN §2.5, §3.6).
//! Each tick while glasses are on: sysinfo CPU/RAM, NVML GPU (init once; skipped on battery
//! systems), foreground process name, fullscreen-foreground suppression of lenses, then
//! emit `og://hud` to the primary lens.

use crate::types::*;
use crate::{ai, capture, grab, settings, state};
use nvml_wrapper::{error::NvmlError, Nvml};
use std::sync::atomic::Ordering;
use std::time::Duration;
use sysinfo::System;
use tauri::AppHandle;

const GIB: f64 = (1u64 << 30) as f64;

/// NVML is tried at most once, lazily on the first tick with glasses on.
enum Gpu {
    Untried,
    On(Box<Nvml>, String),
    Off,
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut sys = System::new();
        // CPU usage is a diff between refreshes; the 1 s tick is well above MINIMUM_CPU_UPDATE_INTERVAL.
        sys.refresh_cpu_usage();
        let mut gpu = Gpu::Untried;
        let mut day = None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if state::st(&app).glasses_on.load(Ordering::Relaxed) {
                tick(&app, &mut sys, &mut gpu, &mut day);
            }
        }
    });
}

fn tick(app: &AppHandle, sys: &mut System, gpu: &mut Gpu, day: &mut Option<chrono::NaiveDate>) {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let (cpu, ram_used_gb, ram_total_gb) =
        (sys.global_cpu_usage() as f64, sys.used_memory() as f64 / GIB, sys.total_memory() as f64 / GIB);
    let gpu = sample_gpu(gpu);
    let fg = grab::foreground_source().filter(|s| !crate::windows::is_own_window(s.hwnd));
    update_suppression(app, fg.as_ref());
    let p = settings::active_provider(&state::settings(app));
    // usage.json is only re-read at start and when the local date rolls over.
    let today = chrono::Local::now().date_naive();
    let tokens = (*day != Some(today)).then(|| {
        *day = Some(today);
        settings::usage_today(app)
    });
    hud_update(app, |h| {
        (h.cpu, h.ram_used_gb, h.ram_total_gb, h.gpu) = (cpu, ram_used_gb, ram_total_gb, gpu);
        // While one of our windows is in front, keep showing the last outside app.
        if let Some(s) = fg {
            h.app = Some(s.process).filter(|p| !p.is_empty());
        }
        (h.provider, h.model) = (p.name, p.model);
        if let Some(t) = tokens {
            h.tokens_today = t;
        }
    });
}

fn sample_gpu(gpu: &mut Gpu) -> Option<GpuStat> {
    if matches!(gpu, Gpu::Untried) {
        *gpu = match (!has_battery()).then(init_nvml) {
            Some(Ok((nvml, name))) => Gpu::On(nvml, name),
            Some(Err(e)) => {
                log::info!("NVML unavailable, GPU stats hidden: {e}");
                Gpu::Off
            }
            None => Gpu::Off,
        };
    }
    let Gpu::On(nvml, name) = gpu else { return None };
    match read_gpu(nvml, name) {
        Ok(s) => Some(s),
        Err(e) => {
            log::info!("NVML read failed, GPU stats hidden: {e}");
            *gpu = Gpu::Off;
            None
        }
    }
}

fn init_nvml() -> Result<(Box<Nvml>, String), NvmlError> {
    let nvml = Box::new(Nvml::init()?);
    let name = nvml.device_by_index(0)?.name()?;
    Ok((nvml, name))
}

fn read_gpu(nvml: &Nvml, name: &str) -> Result<GpuStat, NvmlError> {
    let dev = nvml.device_by_index(0)?;
    let util = dev.utilization_rates()?;
    let mem = dev.memory_info()?;
    Ok(GpuStat {
        name: name.to_string(),
        util: util.gpu as f64,
        vram_used_gb: mem.used as f64 / GIB,
        vram_total_gb: mem.total as f64 / GIB,
    })
}

/// Polling NVML keeps an Optimus dGPU awake, so laptops (any battery) get no GPU block.
fn has_battery() -> bool {
    use ::windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    // BatteryFlag 128 = no system battery, 255 = unknown.
    unsafe { GetSystemPowerStatus(&mut s) }.is_ok() && s.BatteryFlag != 128 && s.BatteryFlag != 255
}

/// Hide the lens whose monitor is fully covered by the (non-own, non-shell) foreground window,
/// unless that lens owns the active session or is selecting; un-hide every other lens.
fn update_suppression(app: &AppHandle, fg: Option<&SourceInfo>) {
    // A hotkey flow (or its toast) just un-suppressed its lens; don't hide it under it.
    if crate::actions::is_busy(app) || crate::actions::recently_toasted(app) {
        return;
    }
    let lenses = crate::windows::lenses(app);
    let target = fg
        .filter(|s| !matches!(grab::class_name(s.hwnd).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd"))
        .and_then(|s| lenses.iter().find(|l| l.bounds == s.rect))
        .map(|l| l.label.as_str())
        .filter(|&label| {
            ai::active_meta(app).is_none_or(|m| m.lens != label)
                && capture::selecting_lens(app).is_none_or(|l| l != label)
        });
    for l in &lenses {
        let want = target == Some(l.label.as_str());
        if l.suppressed != want {
            crate::windows::set_lens_suppressed(app, &l.label, want);
        }
    }
}

/// Mutate the HUD state and emit it to the primary lens. The only writer of `og://hud`.
pub fn hud_update(app: &AppHandle, f: impl FnOnce(&mut HudState)) {
    let hud = {
        let state = state::st(app);
        let mut h = state.hud.lock().unwrap();
        f(&mut h);
        h.clone()
    };
    if let Some(primary) = crate::windows::primary_lens(app) {
        crate::windows::emit_to_window(app, &primary.label, ev::HUD, hud);
    }
}

pub fn set_mode(app: &AppHandle, mode: HudMode) {
    hud_update(app, |h| h.mode = mode);
}

/// After a finished answer: add tokens to usage.json (date-keyed) and update ttft/tps/tokensToday.
pub fn record_usage(app: &AppHandle, output_tokens: u64, ttft_ms: Option<u64>, tps: Option<f64>) {
    let total = settings::add_usage(app, output_tokens);
    hud_update(app, |h| {
        h.tokens_today = total;
        h.ttft_ms = ttft_ms;
        h.tps = tps;
    });
}
