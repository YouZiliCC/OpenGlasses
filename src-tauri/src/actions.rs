//! Orchestration: hotkey actions and the session rules of DESIGN §2.2. All entry points run on
//! worker threads (spawned by hotkeys.rs / cursor.rs / tray.rs), never on the main thread.

use crate::ai::{self, NewSession};
use crate::capture::{self, Shot};
use crate::grab::{self, GrabResult};
use crate::state::{self, st};
use crate::types::*;
use crate::{hotkeys, settings, telemetry, themes, tray, windows};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Grabbing,
    Selecting,
    Scanning,
}

#[derive(Default)]
pub struct FlowState {
    pub phase: Mutex<Phase>,
    /// Last toast; telemetry keeps the lens visible for it (fullscreen suppression).
    pub toast_at: Mutex<Option<Instant>>,
}

/// A busy flow (Grabbing / Scanning). Sets the phase to `next` (Idle unless changed) when
/// dropped, so early returns can't leave the flow stuck.
struct Busy<'a> {
    app: &'a AppHandle,
    next: Phase,
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        set_phase(self.app, self.next);
    }
}

fn set_phase(app: &AppHandle, p: Phase) {
    *st(app).flow.phase.lock().unwrap() = p;
}

/// Enter a busy phase unless another flow is running or SELECT is active (rule 4).
fn try_busy(app: &AppHandle, phase: Phase) -> Option<Busy<'_>> {
    // Lock order: never call into capture while holding the phase lock.
    let selecting = capture::is_selecting(app);
    let state = st(app);
    let mut p = state.flow.phase.lock().unwrap();
    if selecting || matches!(*p, Phase::Grabbing | Phase::Scanning) {
        return None;
    }
    *p = phase;
    Some(Busy {
        app,
        next: Phase::Idle,
    })
}

/// A Grabbing/Scanning flow is running (telemetry must not hide its lens meanwhile).
pub(crate) fn is_busy(app: &AppHandle) -> bool {
    matches!(
        *st(app).flow.phase.lock().unwrap(),
        Phase::Grabbing | Phase::Scanning
    )
}

fn glasses_on(app: &AppHandle) -> bool {
    st(app).glasses_on.load(Ordering::SeqCst)
}

fn foreground_is_ours() -> bool {
    windows::is_own_window(grab::foreground_hwnd())
}

/// Toasts "NO API KEY" and returns false if the active provider can't be used.
fn check_provider(app: &AppHandle) -> bool {
    let ok = settings::provider_usable(&settings::active_provider(&state::settings(app)));
    if !ok {
        toast(app, "NO API KEY");
    }
    ok
}

/// Cursor (physical) and the lens under it, un-suppressed so the session can show.
fn cursor_lens(app: &AppHandle) -> Option<((i32, i32), LensInfo)> {
    let (x, y) = windows::cursor_pos();
    let lens = windows::lens_at(app, x, y)?;
    if lens.suppressed {
        windows::set_lens_suppressed(app, &lens.label, false);
    }
    Some(((x, y), lens))
}

/// Start a session unless the glasses were taken off meanwhile. Returns true if started.
fn start(app: &AppHandle, ns: NewSession) -> bool {
    if !glasses_on(app) {
        if let Some(g) = &ns.glass {
            capture::drop_frame(app, &g.url);
        }
        return false;
    }
    ai::start(app, ns);
    true
}

fn cancel_select(app: &AppHandle) {
    if let Some(label) = capture::selecting_lens(app) {
        capture::finish_select(app, &label, None);
        telemetry::set_mode(app, HudMode::Standby);
    }
}

/// Keep head and tail of a long selection (DESIGN §3.3 step 5).
fn clip_middle(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max / 2).collect();
    let tail: String = text.chars().skip(n - max / 2).collect();
    format!("{head}\n…\n{tail}")
}

/// After setup: register hotkeys (toast conflicts), wait for the primary lens to be ready then
/// play Lens-On (if enabled), and on first run with no usable provider open Settings.
pub fn startup(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let s = state::settings(&app);
        let errors = hotkeys::register_all(&app, &s, true);
        let t0 = Instant::now();
        while !windows::primary_lens(&app).is_some_and(|l| l.ready)
            && t0.elapsed() < Duration::from_secs(8)
        {
            std::thread::sleep(Duration::from_millis(50));
        }
        toast_hotkey_errors(&app, &s, &errors);
        if s.lens_on {
            capture::play_lens_on(&app);
        }
        if !settings::provider_usable(&settings::active_provider(&s)) {
            if s.lens_on {
                // Let the theme's boot stage finish first (it may wait ~1 s for the theme CSS).
                std::thread::sleep(boot_duration(&app, &s) + Duration::from_millis(1150));
            }
            tauri::async_runtime::spawn(windows::open_settings(app.clone()));
        }
    });
}

/// Length of the active theme's boot stage at the current speed (DESIGN §10.4/§10.5).
fn boot_duration(app: &AppHandle, s: &Settings) -> Duration {
    let ts = themes::list(app).themes;
    let boot_ms = ts
        .iter()
        .find(|t| t.id == s.theme)
        .or_else(|| ts.iter().find(|t| t.id == "terminal"))
        .or(ts.first())
        .map_or(1000, |t| t.boot_ms);
    let k = match s.anim_speed {
        AnimSpeed::Fast => 0.6,
        AnimSpeed::Normal => 1.0,
        AnimSpeed::Slow => 1.6,
    };
    Duration::from_millis((boot_ms as f64 * k) as u64)
}

fn toast_hotkey_errors(app: &AppHandle, s: &Settings, errors: &BTreeMap<HotkeyAction, String>) {
    if errors.is_empty() {
        return;
    }
    let text = errors
        .iter()
        .map(|(a, e)| {
            let combo = s.hotkeys.get(a).map_or("?", String::as_str).to_uppercase();
            if e == hotkeys::INVALID {
                format!("HOTKEY {combo} INVALID")
            } else {
                format!("HOTKEY {combo} IN USE")
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    toast(app, &text);
}

/// Dispatch one hotkey action according to §2.2 rules 2–4 (and ignore while Grabbing).
pub fn on_hotkey(app: &AppHandle, action: HotkeyAction) {
    match action {
        HotkeyAction::Toggle => {
            if !is_busy(app) {
                set_glasses(app, !glasses_on(app));
            }
        }
        _ if !glasses_on(app) => {}
        HotkeyAction::Explain => on_explain(app),
        HotkeyAction::Capture => on_capture(app),
        HotkeyAction::Scan => on_scan(app),
    }
}

/// Alt+Q (rule 2).
fn on_explain(app: &AppHandle) {
    if foreground_is_ours() {
        return;
    }
    let Some(_busy) = try_busy(app, Phase::Grabbing) else {
        return;
    };
    if !check_provider(app) {
        return;
    }
    telemetry::set_mode(app, HudMode::Grabbing);
    let started = match grab::grab_selection() {
        GrabResult::Text {
            text,
            anchor,
            source,
        } => explain_text(app, text, anchor, source),
        GrabResult::Empty => {
            match ai::active_meta(app) {
                Some(m) => {
                    ai::dismiss(app, &m.id);
                }
                None => toast(app, "NO SELECTION"),
            }
            false
        }
        GrabResult::Toast(code) => {
            toast(app, code);
            false
        }
        GrabResult::Ignore => false,
    };
    if !started {
        telemetry::set_mode(app, HudMode::Standby);
    }
}

fn explain_text(
    app: &AppHandle,
    text: String,
    anchor: Option<PhysRect>,
    source: SourceInfo,
) -> bool {
    if let Some(m) = ai::active_meta(app) {
        if m.kind == Kind::Text && !m.errored && m.source_text == text {
            ai::dismiss(app, &m.id);
            return false;
        }
    }
    let Some(((cx, cy), lens)) = cursor_lens(app) else {
        return false;
    };
    let glass = capture::grab_monitor(app, &lens)
        .ok()
        .and_then(|shot| capture::glass_from(app, &shot, &lens));
    let cursor = lens.to_css_point(cx, cy);
    let anchor = anchor
        .and_then(|a| a.intersect(&lens.bounds))
        .map(|r| lens.to_css(r))
        .unwrap_or(Rect {
            x: cursor.x - 12.0,
            y: cursor.y - 9.0,
            w: 24.0,
            h: 18.0,
        });
    start(
        app,
        NewSession {
            kind: Kind::Text,
            lens,
            anchor: Some(anchor),
            cursor,
            glass,
            preview: clip_middle(&text, 4000),
            source_text: text,
            image: None,
            source: Some(source),
        },
    )
}

/// Alt+W (rule 3): cancel SELECT, or dismiss and freeze the cursor's monitor.
fn on_capture(app: &AppHandle) {
    if capture::is_selecting(app) {
        cancel_select(app);
        return;
    }
    let Some(mut busy) = try_busy(app, Phase::Grabbing) else {
        return;
    };
    ai::dismiss_active(app);
    if !check_provider(app) {
        return;
    }
    let Some((_, lens)) = cursor_lens(app) else {
        return;
    };
    telemetry::set_mode(app, HudMode::Select);
    match capture::begin_select(app, &lens) {
        Ok(()) => busy.next = Phase::Selecting,
        Err(e) => {
            log::warn!("begin_select: {e}");
            toast(app, "CAPTURE FAILED");
            telemetry::set_mode(app, HudMode::Standby);
        }
    }
}

/// Alt+E (rule 3, §2.6 step 1).
fn on_scan(app: &AppHandle) {
    if foreground_is_ours() {
        return;
    }
    let Some(_busy) = try_busy(app, Phase::Scanning) else {
        return;
    };
    if let Some(m) = ai::active_meta(app) {
        ai::dismiss(app, &m.id);
        if m.kind == Kind::Scan {
            return;
        }
    }
    if !check_provider(app) {
        return;
    }
    telemetry::set_mode(app, HudMode::Scan);
    if !scan_start(app) {
        telemetry::set_mode(app, HudMode::Standby);
    }
}

fn scan_start(app: &AppHandle) -> bool {
    let source = grab::foreground_source();
    let Some(((cx, cy), lens)) = cursor_lens(app) else {
        return false;
    };
    let shot = match capture::grab_monitor(app, &lens) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("scan capture: {e}");
            toast(app, "CAPTURE FAILED");
            return false;
        }
    };
    // Foreground window ∩ cursor monitor, else the whole monitor.
    let region = source
        .as_ref()
        .filter(|s| {
            !windows::is_own_window(s.hwnd)
                && !matches!(
                    grab::class_name(s.hwnd).as_str(),
                    "Progman" | "WorkerW" | "Shell_TrayWnd"
                )
        })
        .and_then(|s| s.rect.intersect(&lens.bounds))
        .filter(|r| r.w >= 64 && r.h >= 64)
        .unwrap_or(lens.bounds);
    let Some(image) = encode(app, &shot, region) else {
        return false;
    };
    let preview = match &source {
        Some(s) if !s.process.is_empty() => format!("[scan] {}", s.process),
        _ => "[scan]".to_string(),
    };
    start(
        app,
        NewSession {
            kind: Kind::Scan,
            anchor: Some(lens.to_css(region)),
            cursor: lens.to_css_point(cx, cy),
            glass: capture::glass_from(app, &shot, &lens),
            lens,
            preview,
            source_text: String::new(),
            image: Some(image),
            source,
        },
    )
}

/// Crop + protected-content check + model encoding.
fn encode(app: &AppHandle, shot: &Shot, rect: PhysRect) -> Option<ImagePayload> {
    let img = capture::crop(shot, rect)?;
    if capture::is_all_black(&img) {
        toast(app, "PROTECTED CONTENT");
        return None;
    }
    capture::encode_for_model(&img, rect)
        .map_err(|e| log::warn!("encode: {e}"))
        .ok()
}

/// Esc pressed during SELECT -> cancel.
pub fn on_escape(app: &AppHandle) {
    cancel_select(app);
}

/// Show a short warning next to the cursor (`og://toast` to the cursor's lens).
pub fn toast(app: &AppHandle, text: &str) {
    let (x, y) = windows::cursor_pos();
    if let Some(lens) = windows::lens_at(app, x, y) {
        *st(app).flow.toast_at.lock().unwrap() = Some(Instant::now());
        if lens.suppressed {
            windows::set_lens_suppressed(app, &lens.label, false);
        }
        let p = lens.to_css_point(x, y);
        windows::emit_to_window(
            app,
            &lens.label,
            ev::TOAST,
            ToastEvent {
                text: text.to_string(),
                x: p.x,
                y: p.y,
            },
        );
    }
}

/// A toast was shown within the last 2 s (it needs its lens visible).
pub(crate) fn recently_toasted(app: &AppHandle) -> bool {
    st(app).flow.toast_at.lock().unwrap().is_some_and(|t| t.elapsed() < Duration::from_secs(2))
}

/// Rail rules apply to Rail sessions, but scan results always show as a card (DESIGN §2.6).
fn is_rail(m: &ai::ActiveMeta) -> bool {
    m.style == Style::Rail && m.kind != Kind::Scan
}

/// From cursor.rs: left-button down outside the panel (rule 6).
pub fn on_click_outside(app: &AppHandle) {
    if let Some(m) = ai::active_meta(app) {
        if !m.pinned && !is_rail(&m) {
            ai::dismiss(app, &m.id);
        }
    }
}

/// From cursor.rs: the active session's source lost foreground / moved / minimized (rule 7).
pub fn on_source_changed(app: &AppHandle) {
    if let Some(m) = ai::active_meta(app) {
        if is_rail(&m) || m.pinned {
            ai::mark_stale(app);
        } else {
            ai::dismiss(app, &m.id);
        }
    }
}

/// From capture.rs: the user finished a region selection. `rect` is physical px, `shot` the frozen frame.
pub fn on_selection(app: &AppHandle, lens: LensInfo, shot: Shot, rect: PhysRect) {
    // SELECT is over; stay busy until the session has started.
    set_phase(app, Phase::Grabbing);
    let _busy = Busy {
        app,
        next: Phase::Idle,
    };
    let Some(image) = encode(app, &shot, rect) else {
        telemetry::set_mode(app, HudMode::Standby);
        return;
    };
    let (cx, cy) = windows::cursor_pos();
    // Lenses never take activation, so the foreground is still the user's window.
    let source = grab::foreground_source().filter(|s| !windows::is_own_window(s.hwnd));
    let started = start(
        app,
        NewSession {
            kind: Kind::Image,
            anchor: Some(lens.to_css(rect)),
            cursor: lens.to_css_point(cx, cy),
            glass: capture::glass_from(app, &shot, &lens),
            lens,
            preview: "[screenshot]".to_string(),
            source_text: String::new(),
            image: Some(image),
            source,
        },
    );
    if !started {
        telemetry::set_mode(app, HudMode::Standby);
    }
}

/// Glasses on/off (rule 10). Also used by the tray.
pub fn set_glasses(app: &AppHandle, on: bool) {
    if st(app).glasses_on.swap(on, Ordering::SeqCst) == on {
        return;
    }
    let s = state::settings(app);
    if on {
        windows::set_overlays_visible(app, true);
        let errors = hotkeys::register_all(app, &s, true);
        tray::set_glasses_item(app, true);
        let _ = app.emit(ev::VISIBILITY, VisibilityEvent { on: true });
        toast_hotkey_errors(app, &s, &errors);
        if s.lens_on {
            capture::play_lens_on(app);
        }
    } else {
        cancel_select(app);
        ai::dismiss_active(app);
        windows::set_overlays_visible(app, false);
        hotkeys::register_all(app, &s, false);
        tray::set_glasses_item(app, false);
        let _ = app.emit(ev::VISIBILITY, VisibilityEvent { on: false });
    }
}

/// `save_settings`: persist, re-register hotkeys, apply content protection + glass mode,
/// broadcast `og://settings`; returns hotkey errors.
pub fn apply_settings(app: &AppHandle, s: Settings) -> Result<SaveSettingsResult, String> {
    let old = state::settings(app);
    settings::save(app, &s)?;
    let hotkey_errors = hotkeys::register_all(app, &s, glasses_on(app));
    windows::set_content_protected_all(app, s.exclude_from_capture);
    windows::apply_glass_mode(app, s.glass);
    let _ = app.emit(ev::SETTINGS, &s);
    // A new theme (or speed) replays the boot so the change is shown right away. The lens
    // mounts the stage only once the new theme's CSS has loaded (og://settings goes first).
    if (old.theme != s.theme || old.anim_speed != s.anim_speed) && s.lens_on && glasses_on(app) {
        let app = app.clone();
        std::thread::spawn(move || capture::play_lens_on(&app));
    }
    Ok(SaveSettingsResult { hotkey_errors })
}
