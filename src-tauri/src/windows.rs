//! Overlay windows: lens-<n> per monitor + one reused panel + on-demand settings.
//! DESIGN §3.1 "overlay rules" are mandatory here: after creation, overlays are shown,
//! hidden, raised and made interactive ONLY through the Win32 helpers in this file
//! (via `run_on_main_thread`). Never call Tauri show/hide/set_focus/set_focusable/
//! set_always_on_top/set_ignore_cursor_events/set_skip_taskbar/is_visible on them.

use crate::state::{self, st};
use crate::types::*;
use ::windows::Win32::Foundation::{HWND, POINT, RECT};
use ::windows::Win32::Graphics::Dwm::DwmFlush;
use ::windows::Win32::System::Threading::GetCurrentProcessId;
use ::windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
    IsWindowVisible, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE,
    HWND_TOPMOST, SET_WINDOW_POS_FLAGS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WINDOW_EX_STYLE, WS_EX_APPWINDOW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const PANEL: &str = "panel";
const SETTINGS: &str = "settings";
/// Panel size before its first request (CSS px).
const PANEL_INIT: (f64, f64) = (420.0, 200.0);

#[derive(Default)]
pub struct WinState {
    pub lenses: Mutex<Vec<LensInfo>>,
    pub panel: Mutex<PanelInfo>,
    /// Source of `lens-<n>` labels; never reused.
    pub counter: AtomicU32,
    /// `frontend_ready` received from the current settings window.
    settings_ready: AtomicBool,
    /// Captures in flight inside `without_overlays_in_capture`. The lock also keeps the
    /// protect/restore posts to the main thread in order.
    capture_depth: Mutex<u32>,
}

/// Create one lens per monitor (visible(false), init_overlay) and the hidden panel, then
/// start the 2 s monitor watcher (rebuild lenses when the set/scale/work area changes:
/// dismiss the active session, build new lenses, destroy old ones; postpone while selecting).
/// Lenses are shown when their `frontend_ready` arrives (if glasses are on).
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let infos = new_lens_infos(app, &monitors(app));
    *win(app).lenses.lock().unwrap() = infos.clone();
    for l in &infos {
        create_overlay(app, &l.label, true, l.bounds)?;
    }

    let init = match primary_lens(app) {
        Some(l) => {
            let wa = l.to_css(l.work_area);
            l.to_phys(Rect { x: wa.x, y: wa.y, w: PANEL_INIT.0, h: PANEL_INIT.1 })
        }
        None => PhysRect { x: 0, y: 0, w: PANEL_INIT.0 as i32, h: PANEL_INIT.1 as i32 },
    };
    create_overlay(app, PANEL, false, init)?;
    apply_glass_mode(app, state::settings(app).glass);

    let h = app.clone();
    std::thread::Builder::new().name("og-monitors".into()).spawn(move || watch_monitors(h))?;
    Ok(())
}

/// Handles the `frontend_ready` command for any window: marks lens/panel ready, shows a lens
/// (if glasses on and not suppressed), and returns the snapshot it needs to render.
pub fn frontend_ready(app: &AppHandle, label: &str) -> InitState {
    let glasses_on = st(app).glasses_on.load(Ordering::SeqCst);
    let mut init = InitState {
        label: label.to_string(),
        is_primary: false,
        work_area: None,
        settings: state::settings(app),
        hud: None,
        active: None,
        panel: None,
        glasses_on,
    };
    if label == PANEL {
        let mut p = win(app).panel.lock().unwrap();
        p.ready = true;
        init.panel = p.event.clone();
        drop(p);
        init.active = crate::ai::snapshot(app);
    } else if label == SETTINGS {
        win(app).settings_ready.store(true, Ordering::SeqCst);
    } else {
        let lens = {
            let mut v = win(app).lenses.lock().unwrap();
            v.iter_mut().find(|l| l.label == label).map(|l| {
                l.ready = true;
                l.clone()
            })
        };
        let Some(lens) = lens else { return init };
        if glasses_on && !lens.suppressed {
            show_lens(app, label);
        }
        init.is_primary = lens.primary;
        init.work_area = Some(lens.to_css(lens.work_area));
        if lens.primary {
            init.hud = Some(st(app).hud.lock().unwrap().clone());
        }
        init.active = crate::ai::snapshot(app).filter(|s| s.lens == label);
    }
    init
}

pub fn lenses(app: &AppHandle) -> Vec<LensInfo> {
    win(app).lenses.lock().unwrap().clone()
}

pub fn lens_by_label(app: &AppHandle, label: &str) -> Option<LensInfo> {
    win(app).lenses.lock().unwrap().iter().find(|l| l.label == label).cloned()
}

/// Lens of the monitor containing the physical point (nearest monitor if none contains it).
pub fn lens_at(app: &AppHandle, x: i32, y: i32) -> Option<LensInfo> {
    let v = win(app).lenses.lock().unwrap();
    let dist = |r: &PhysRect| {
        let dx = (r.x - x).max(x - (r.x + r.w - 1)).max(0) as i64;
        let dy = (r.y - y).max(y - (r.y + r.h - 1)).max(0) as i64;
        dx * dx + dy * dy
    };
    v.iter()
        .find(|l| l.bounds.contains(x, y))
        .or_else(|| v.iter().min_by_key(|l| dist(&l.bounds)))
        .cloned()
}

pub fn primary_lens(app: &AppHandle) -> Option<LensInfo> {
    let v = win(app).lenses.lock().unwrap();
    v.iter().find(|l| l.primary).or(v.first()).cloned()
}

/// GetCursorPos, physical px.
pub fn cursor_pos() -> (i32, i32) {
    try_cursor_pos().unwrap_or_default()
}

/// GetCursorPos; None when it fails (e.g. the secure desktop is active).
pub(crate) fn try_cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((p.x, p.y))
}

/// Emit to one window by label, but only once that window reported `frontend_ready`.
pub fn emit_to_window<S: Serialize + Clone>(app: &AppHandle, label: &str, event: &str, payload: S) {
    let w = win(app);
    let ready = match label {
        PANEL => w.panel.lock().unwrap().ready,
        SETTINGS => w.settings_ready.load(Ordering::SeqCst),
        _ => w.lenses.lock().unwrap().iter().any(|l| l.label == label && l.ready),
    };
    if ready {
        if let Err(e) = app.emit_to(label, event, payload) {
            log::warn!("emit {event} to {label}: {e}");
        }
    }
}

/// Emit to the panel (if ready).
pub fn emit_to_panel<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    emit_to_window(app, PANEL, event, payload);
}

/// Glasses on/off: show (raw, no-activate) or hide every lens; when off also hide the panel.
pub fn set_overlays_visible(app: &AppHandle, on: bool) {
    for l in lenses(app).into_iter().filter(|l| l.ready) {
        if on && !l.suppressed {
            show_lens(app, &l.label);
        } else {
            hide_window(app, &l.label);
        }
    }
    if !on {
        hide_panel(app, 0);
    }
}

/// Hide/show one lens because a fullscreen app owns its monitor (telemetry.rs decides).
pub fn set_lens_suppressed(app: &AppHandle, label: &str, suppressed: bool) {
    let ready = {
        let mut v = win(app).lenses.lock().unwrap();
        let Some(l) = v.iter_mut().find(|l| l.label == label) else { return };
        if l.suppressed == suppressed {
            return;
        }
        l.suppressed = suppressed;
        l.ready
    };
    if !ready {
        return;
    }
    if suppressed {
        hide_window(app, label);
    } else if st(app).glasses_on.load(Ordering::SeqCst) {
        show_lens(app, label);
    }
}

/// Toggle WS_EX_TRANSPARENT|WS_EX_LAYERED only (keep TOOLWINDOW/NOACTIVATE); when `on`,
/// also re-assert HWND_TOPMOST with SWP_FRAMECHANGED|SWP_NOACTIVATE. Used for SELECT only.
pub fn set_lens_interactive(app: &AppHandle, label: &str, on: bool) {
    let label = label.to_string();
    main_async(app, move |app| {
        let Some(h) = hwnd_of(app, &label) else { return };
        let click_through = WS_EX_TRANSPARENT | WS_EX_LAYERED;
        let ex = ex_style(h);
        set_ex_style(h, if on { ex & !click_through } else { ex | click_through });
        if on {
            set_pos(h, Some(HWND_TOPMOST), SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED | SWP_NOACTIVATE);
        }
    });
}

/// Apply `set_content_protected(on)` to every lens and the panel (WDA_EXCLUDEFROMCAPTURE).
pub fn set_content_protected_all(app: &AppHandle, on: bool) {
    let depth = win(app).capture_depth.lock().unwrap();
    // A capture in flight restores from the settings when it ends.
    if *depth == 0 || on {
        protect(app, on);
    }
}

/// Run a capture `f` on the calling worker thread with our overlays excluded from it.
/// If settings.excludeFromCapture is already on, just runs `f`. Otherwise temporarily
/// protects all overlays on the main thread, waits for that + DwmFlush, runs `f`, restores.
/// Must not be called from the main thread.
pub fn without_overlays_in_capture<T>(app: &AppHandle, f: impl FnOnce() -> T) -> T {
    if state::settings(app).exclude_from_capture {
        return f();
    }
    let w = win(app);
    let (tx, rx) = mpsc::channel();
    {
        let mut depth = w.capture_depth.lock().unwrap();
        *depth += 1;
        let h = app.clone();
        // On the main thread set_content_protected runs inline, so `tx` fires once it is applied.
        let _ = app.run_on_main_thread(move || {
            protect(&h, true);
            let _ = tx.send(());
        });
    }
    let _ = rx.recv();
    unsafe {
        let _ = DwmFlush();
    }
    let out = f();
    let exclude = state::settings(app).exclude_from_capture;
    let mut depth = w.capture_depth.lock().unwrap();
    *depth -= 1;
    if *depth == 0 && !exclude {
        protect(app, false);
    }
    out
}

/// Lens asked to show the panel (`show_panel` command). Store the request (box in that
/// lens's CSS px), emit `og://panel`, and wait for the panel's first `panel_set_height`
/// for this id before placing + showing it (SW no-activate). Rail requests come the same way.
pub fn request_panel(app: &AppHandle, lens_label: &str, id: &str, r#box: Rect, grow_up: bool, item_key: Option<String>) {
    let Some(meta) = crate::ai::active_meta(app).filter(|m| m.id == id && m.lens == lens_label) else {
        return;
    };
    let Some(lens) = lens_by_label(app, lens_label) else { return };
    let pe = PanelEvent {
        id: id.to_string(),
        // Scan results are always a card list, whatever the default style (DESIGN §2.6).
        mode: if meta.style == Style::Rail && meta.kind != Kind::Scan { PanelMode::Rail } else { PanelMode::Card },
        r#box,
        grow_up,
        item_key,
    };
    let mut p = win(app).panel.lock().unwrap();
    p.event = Some(pe.clone());
    p.lens = Some(lens.label.clone());
    if !p.visible {
        // Pre-place while hidden so the panel lays out at the target width and monitor DPI.
        let r = clamp_to(lens.to_phys(r#box), lens.work_area);
        main_async(app, move |app| {
            if let Some(h) = hwnd_of(app, PANEL) {
                place(h, r);
            }
        });
    }
    drop(p);
    emit_to_panel(app, ev::PANEL, pe);
}

/// Panel reported its content height (CSS px). Ignore if `id` is not the current request.
/// height = min(css_h, box.h); top = growUp ? box.bottom - height : box.y; convert with the
/// lens scale, clamp to the work area, `place` + (first time) `show`.
pub fn panel_set_height(app: &AppHandle, id: &str, css_h: f64) {
    if !css_h.is_finite() || !st(app).glasses_on.load(Ordering::SeqCst) {
        return;
    }
    let (pe, lens_label) = {
        let p = win(app).panel.lock().unwrap();
        match (&p.event, &p.lens) {
            (Some(e), Some(l)) if e.id == id => (e.clone(), l.clone()),
            _ => return,
        }
    };
    let Some(lens) = lens_by_label(app, &lens_label) else { return };
    let b = pe.r#box;
    let h = css_h.clamp(1.0, b.h.max(1.0));
    let top = if pe.grow_up { b.y + b.h - h } else { b.y };
    let r = clamp_to(lens.to_phys(Rect { x: b.x, y: top, w: b.w, h }), lens.work_area);

    let mut p = win(app).panel.lock().unwrap();
    // The request may have been replaced or hidden meanwhile.
    if p.event.as_ref() != Some(&pe) || (p.visible && p.phys == Some(r)) {
        return;
    }
    let first = !p.visible;
    p.visible = true;
    p.phys = Some(r);
    main_async(app, move |app| {
        if let Some(h) = hwnd_of(app, PANEL) {
            place(h, r);
            if first {
                set_pos(h, Some(HWND_TOPMOST), SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
            }
        }
    });
}

/// Hide the panel. If the panel is the foreground window and `restore_focus_to != 0`,
/// SetForegroundWindow(restore_focus_to) first.
pub fn hide_panel(app: &AppHandle, restore_focus_to: isize) {
    let mut p = win(app).panel.lock().unwrap();
    p.visible = false;
    p.event = None;
    p.lens = None;
    p.phys = None;
    main_async(app, move |app| {
        let Some(h) = hwnd_of(app, PANEL) else { return };
        unsafe {
            // Hiding the active window would hand activation to an arbitrary window.
            if restore_focus_to != 0 && GetForegroundWindow() == h {
                let _ = SetForegroundWindow(HWND(restore_focus_to as _));
            }
            let _ = ShowWindow(h, SW_HIDE);
        }
    });
}

/// Dismiss path: let the panel's 150 ms CSS fade play, then hide — unless a newer session
/// took the panel over meanwhile (its request re-places the still-visible window).
pub(crate) fn hide_panel_after_fade(app: &AppHandle, id: &str, restore_focus_to: isize) {
    {
        let mut p = win(app).panel.lock().unwrap();
        if p.event.as_ref().is_some_and(|e| e.id != id) {
            return;
        }
        // Late height reports for the dismissed session are ignored from now on.
        p.event = None;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        if win(&app).panel.lock().unwrap().event.is_none() {
            hide_panel(&app, restore_focus_to);
        }
    });
}

/// Physical rect of the panel while visible.
pub fn panel_rect(app: &AppHandle) -> Option<PhysRect> {
    let p = win(app).panel.lock().unwrap();
    if p.visible {
        p.phys
    } else {
        None
    }
}

/// Glass mode for the panel: Acrylic -> `set_effects(Acrylic)`, Snapshot -> clear effects.
pub fn apply_glass_mode(app: &AppHandle, mode: GlassMode) {
    // set_effects goes straight to DWM (window-vibrancy), not through tao's window flags.
    let Some(w) = app.get_webview_window(PANEL) else { return };
    let effects: Option<WindowEffectsConfig> = match mode {
        GlassMode::Acrylic => Some(EffectsBuilder::new().effect(Effect::Acrylic).build()),
        GlassMode::Snapshot => None,
    };
    if let Err(e) = w.set_effects(effects) {
        log::warn!("panel effects: {e}");
    }
}

/// Create (or focus) the normal "settings" window. Never call on the main thread.
pub async fn open_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.unminimize();
        let _ = w.set_focus();
        return;
    }
    win(&app).settings_ready.store(false, Ordering::SeqCst);
    let built = WebviewWindowBuilder::new(&app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title("OpenGlasses Settings")
        .inner_size(820.0, 600.0)
        .resizable(true)
        .center()
        .build();
    if let Err(e) = built {
        log::error!("settings window: {e}");
    }
}

/// True if `hwnd` belongs to this process.
pub fn is_own_window(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(hwnd as _), Some(&mut pid));
        pid != 0 && pid == GetCurrentProcessId()
    }
}

// ---------------------------------------------------------------- monitors

/// What the watcher compares.
#[derive(PartialEq)]
struct Mon {
    name: String,
    bounds: PhysRect,
    work: PhysRect,
    scale: f64,
    primary: bool,
}

fn monitors(app: &AppHandle) -> Vec<Mon> {
    let mut v: Vec<Mon> = app
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            let (p, s, wa) = (m.position(), m.size(), m.work_area());
            Mon {
                name: m.name().cloned().unwrap_or_default(),
                bounds: PhysRect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 },
                work: PhysRect {
                    x: wa.position.x,
                    y: wa.position.y,
                    w: wa.size.width as i32,
                    h: wa.size.height as i32,
                },
                scale: m.scale_factor(),
                // The primary monitor always sits at the virtual-desktop origin (= tao's primary_monitor).
                primary: p.x == 0 && p.y == 0,
            }
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn current_mons(app: &AppHandle) -> Vec<Mon> {
    let mut v: Vec<Mon> = lenses(app)
        .into_iter()
        .map(|l| Mon { name: l.monitor, bounds: l.bounds, work: l.work_area, scale: l.scale, primary: l.primary })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn new_lens_infos(app: &AppHandle, mons: &[Mon]) -> Vec<LensInfo> {
    mons.iter()
        .map(|m| LensInfo {
            label: format!("lens-{}", win(app).counter.fetch_add(1, Ordering::SeqCst)),
            monitor: m.name.clone(),
            bounds: m.bounds,
            work_area: m.work,
            scale: m.scale,
            primary: m.primary,
            ready: false,
            suppressed: false,
        })
        .collect()
}

fn watch_monitors(app: AppHandle) {
    loop {
        std::thread::sleep(Duration::from_secs(2));
        if crate::capture::is_selecting(&app) {
            continue;
        }
        let mons = monitors(&app);
        // An empty list is transient (display sleep / reconfiguration): keep what we have.
        if mons.is_empty() || mons == current_mons(&app) {
            continue;
        }
        log::info!("monitor layout changed: rebuilding lenses");
        crate::ai::dismiss_active(&app);
        // New infos go in first so a fast `frontend_ready` always finds its lens.
        let new = new_lens_infos(&app, &mons);
        let old = std::mem::replace(&mut *win(&app).lenses.lock().unwrap(), new.clone());
        for l in &new {
            if let Err(e) = create_overlay(&app, &l.label, true, l.bounds) {
                log::error!("create {}: {e}", l.label);
                win(&app).lenses.lock().unwrap().retain(|x| x.label != l.label);
            }
        }
        for l in old {
            if let Some(w) = app.get_webview_window(&l.label) {
                let _ = w.destroy();
            }
        }
    }
}

// ---------------------------------------------------------------- Win32 helpers

fn win(app: &AppHandle) -> &WinState {
    &st(app).inner().win
}

/// Build a lens or the panel with the §3.1 creation parameters, then `init_overlay` + `place`.
fn create_overlay(app: &AppHandle, label: &str, lens: bool, rect: PhysRect) -> tauri::Result<()> {
    let page = if lens { "lens.html" } else { "panel.html" };
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title("OpenGlasses")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .no_redirection_bitmap(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // Undecorated + resizable would turn the edges into resize handles (tao WM_NCHITTEST).
        .resizable(false)
        .visible(false)
        .focused(false)
        .focusable(!lens)
        .content_protected(state::settings(app).exclude_from_capture)
        .build()?;
    let label = label.to_string();
    main_async(app, move |app| {
        if let Some(h) = hwnd_of(app, &label) {
            init_overlay(h, lens);
            place(h, rect);
        }
    });
    Ok(())
}

/// Keep overlays out of Alt+Tab / the taskbar; lenses are also click-through and never activate.
fn init_overlay(h: HWND, lens: bool) {
    let mut ex = (ex_style(h) | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
    if lens {
        ex = ex | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE;
    }
    set_ex_style(h, ex);
    set_pos(h, None, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
}

/// SetWindowPos(HWND_TOPMOST, rect, SWP_NOACTIVATE), verified with GetWindowRect: a move across
/// monitors with different DPI gets resized by WM_DPICHANGED, so set it once more.
fn place(h: HWND, r: PhysRect) {
    for _ in 0..2 {
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), r.x, r.y, r.w, r.h, SWP_NOACTIVATE);
        }
        let mut cur = RECT::default();
        if unsafe { GetWindowRect(h, &mut cur) }.is_err()
            || (cur.left, cur.top, cur.right - cur.left, cur.bottom - cur.top) == (r.x, r.y, r.w, r.h)
        {
            break;
        }
    }
}

fn show_lens(app: &AppHandle, label: &str) {
    let label = label.to_string();
    main_async(app, move |app| {
        let Some(h) = hwnd_of(app, &label) else { return };
        set_pos(h, Some(HWND_TOPMOST), SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        // A lens covers its whole monitor: keep a visible panel above it.
        if let Some(p) = hwnd_of(app, PANEL) {
            if unsafe { IsWindowVisible(p) }.as_bool() {
                set_pos(p, Some(HWND_TOPMOST), SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    });
}

fn hide_window(app: &AppHandle, label: &str) {
    let label = label.to_string();
    main_async(app, move |app| {
        if let Some(h) = hwnd_of(app, &label) {
            unsafe {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
    });
}

fn overlay_labels(app: &AppHandle) -> Vec<String> {
    let mut v: Vec<String> = lenses(app).into_iter().map(|l| l.label).collect();
    v.push(PANEL.to_string());
    v
}

fn protect(app: &AppHandle, on: bool) {
    for l in overlay_labels(app) {
        if let Some(w) = app.get_webview_window(&l) {
            let _ = w.set_content_protected(on);
        }
    }
}

fn clamp_to(r: PhysRect, wa: PhysRect) -> PhysRect {
    let w = r.w.min(wa.w).max(1);
    let h = r.h.min(wa.h).max(1);
    PhysRect {
        x: r.x.min(wa.x + wa.w - w).max(wa.x),
        y: r.y.min(wa.y + wa.h - h).max(wa.y),
        w,
        h,
    }
}

/// Run `f` on the main thread without waiting (inline when already on it). Posts from one
/// thread run in order, so state updated before posting stays consistent with the window.
fn main_async(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let h = app.clone();
    if let Err(e) = app.run_on_main_thread(move || f(&h)) {
        log::warn!("run_on_main_thread: {e}");
    }
}

/// Meant for main-thread closures (inline there; a blocking round-trip anywhere else).
fn hwnd_of(app: &AppHandle, label: &str) -> Option<HWND> {
    app.get_webview_window(label)?.hwnd().ok()
}

fn ex_style(h: HWND) -> WINDOW_EX_STYLE {
    WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32)
}

fn set_ex_style(h: HWND, ex: WINDOW_EX_STYLE) {
    unsafe {
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex.0 as isize);
    }
}

fn set_pos(h: HWND, after: Option<HWND>, flags: SET_WINDOW_POS_FLAGS) {
    unsafe {
        let _ = SetWindowPos(h, after, 0, 0, 0, 0, flags);
    }
}
