//! Screen capture, the `ogframe://` protocol, snapshot glass, Lens-On frames, SELECT,
//! and image encoding for models (DESIGN §2.4, §3.4).

use crate::state::st;
use crate::types::*;
use crate::{actions, hotkeys, telemetry, windows};
use base64::Engine;
use fast_image_resize as fr;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ImageEncoder, RgbaImage};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::http::{header, Request, Response};
use tauri::{AppHandle, UriSchemeContext, UriSchemeResponder, Wry};

const FRAME_BASE: &str = "http://ogframe.localhost/";
const LENS_ON_BUDGET: Duration = Duration::from_millis(150);
const MODEL_LONG_EDGE: u32 = 1568;
const MODEL_PNG_MAX_PX: u32 = 1_500_000;
const MIN_SELECT_CSS: f64 = 8.0;

/// A full-monitor capture held in memory.
pub struct Shot {
    pub img: image::RgbaImage,
    /// Physical rect the image covers (the monitor bounds).
    pub bounds: PhysRect,
}

pub struct SelectState {
    pub lens: String,
    pub shot: Shot,
    /// The `og://freeze` frame shown on `lens`.
    url: String,
}

#[derive(Default)]
pub struct CaptureState {
    /// key (`<uuid>.jpg|png`) -> (mime, bytes) served by `ogframe://`.
    pub frames: Mutex<HashMap<String, (&'static str, Vec<u8>)>>,
    pub select: Mutex<Option<SelectState>>,
}

/// `ogframe://` handler: serve `frames[key]` with Content-Type and `Cache-Control: no-store`; 404 otherwise.
pub fn serve(ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let key = request.uri().path().trim_start_matches('/');
    let hit = st(ctx.app_handle()).capture.frames.lock().unwrap().get(key).cloned();
    let res = Response::builder()
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    let res = match hit {
        Some((mime, bytes)) => res.header(header::CONTENT_TYPE, mime).body(bytes),
        None => res.status(404).body(Vec::new()),
    };
    responder.respond(res.unwrap_or_default());
}

/// Store bytes and return the full URL `http://ogframe.localhost/<key>`.
pub fn put_frame(app: &AppHandle, mime: &'static str, bytes: Vec<u8>) -> String {
    let ext = if mime == "image/png" { "png" } else { "jpg" };
    let key = format!("{}.{ext}", uuid::Uuid::new_v4());
    let url = format!("{FRAME_BASE}{key}");
    st(app).capture.frames.lock().unwrap().insert(key, (mime, bytes));
    url
}

/// Remove a frame by URL or key (no-op if missing).
pub fn drop_frame(app: &AppHandle, url: &str) {
    let key = url.rsplit('/').next().unwrap_or(url);
    st(app).capture.frames.lock().unwrap().remove(key);
}

/// Capture the monitor under the lens centre with xcap (GDI) on the calling worker thread,
/// excluding our overlays (`windows::without_overlays_in_capture`).
pub fn grab_monitor(app: &AppHandle, lens: &LensInfo) -> Result<Shot, String> {
    let b = lens.bounds;
    let mon = xcap::Monitor::from_point(b.x + b.w / 2, b.y + b.h / 2).map_err(|e| e.to_string())?;
    let (x, y) = (mon.x().map_err(|e| e.to_string())?, mon.y().map_err(|e| e.to_string())?);
    let img = windows::without_overlays_in_capture(app, || mon.capture_image()).map_err(|e| e.to_string())?;
    Ok(Shot { bounds: PhysRect { x, y, w: img.width() as i32, h: img.height() as i32 }, img })
}

/// Snapshot glass from a shot: downscale to 1/8, darken/tint, JPEG q70, `put_frame`.
/// Returned w/h = the lens CSS size.
pub fn glass_from(app: &AppHandle, shot: &Shot, lens: &LensInfo) -> Option<Glass> {
    let (w, h) = shot.img.dimensions();
    let small = resize(&shot.img, w / 8, h / 8, fr::FilterType::Box)
        .inspect_err(|e| log::warn!("glass resize: {e}"))
        .ok()?;
    let mut glass = image::imageops::fast_blur(&small, 2.5);
    for px in glass.pixels_mut() {
        for c in &mut px.0[..3] {
            *c = ((*c as u16 * 141) >> 8) as u8; // ×0.55
        }
    }
    let bytes = jpeg(&glass, 70).inspect_err(|e| log::warn!("glass encode: {e}")).ok()?;
    let (cw, ch) = lens.css_size();
    Some(Glass { url: put_frame(app, "image/jpeg", bytes), w: cw, h: ch })
}

/// Lens-On on the primary monitor: capture, 1/32 1/12 1/4 JPEG frames, emit `og://lens-on`
/// with `frames: null` if over the 150 ms budget or on failure. Frames dropped after 5 s.
/// Runs on a worker thread.
pub fn play_lens_on(app: &AppHandle) {
    let Some(lens) = windows::primary_lens(app) else { return };
    let started = Instant::now();
    let frames = match lens_on_frames(app, &lens) {
        Ok(f) if started.elapsed() <= LENS_ON_BUDGET => {
            Some(f.into_iter().map(|b| put_frame(app, "image/jpeg", b)).collect::<Vec<_>>())
        }
        Ok(_) => {
            log::info!("lens-on over budget ({:?}), playing the no-screenshot variant", started.elapsed());
            None
        }
        Err(e) => {
            log::warn!("lens-on capture failed: {e}");
            None
        }
    };
    windows::emit_to_window(app, &lens.label, ev::LENS_ON, LensOnEvent { frames: frames.clone() });
    if let Some(urls) = frames {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            urls.iter().for_each(|u| drop_frame(&app, u));
        });
    }
}

/// JPEG bytes of the 1/32, 1/12 and 1/4 frames (most blurred first).
fn lens_on_frames(app: &AppHandle, lens: &LensInfo) -> Result<Vec<Vec<u8>>, String> {
    let shot = grab_monitor(app, lens)?;
    let (w, h) = shot.img.dimensions();
    // The smaller two are derived from the 1/4 frame: one full-size pass instead of three.
    let quarter = resize(&shot.img, w / 4, h / 4, fr::FilterType::Box)?;
    let mid = resize(&quarter, w / 12, h / 12, fr::FilterType::Box)?;
    let tiny = resize(&quarter, w / 32, h / 32, fr::FilterType::Box)?;
    [&tiny, &mid, &quarter].into_iter().map(|i| jpeg(i, 75)).collect()
}

/// Enter SELECT on `lens`: capture, emit `og://freeze {url,imgW,imgH}` (JPEG q85 full size),
/// hide the panel, `windows::set_lens_interactive(on)`, `hotkeys::set_escape(true)`.
pub fn begin_select(app: &AppHandle, lens: &LensInfo) -> Result<(), String> {
    let shot = grab_monitor(app, lens)?;
    let (img_w, img_h) = shot.img.dimensions();
    let url = put_frame(app, "image/jpeg", jpeg(&shot.img, 85)?);
    let payload = FreezePayload { url: url.clone(), img_w, img_h };
    windows::emit_to_window(app, &lens.label, ev::FREEZE, Some(payload));
    windows::hide_panel(app);
    windows::set_lens_interactive(app, &lens.label, true);
    hotkeys::set_escape(app, true);
    telemetry::set_mode(app, HudMode::Select);
    // Publish last: a concurrent cancel can only take the state after Esc is registered and
    // the click-through change is queued, so its undo is ordered after them.
    *st(app).capture.select.lock().unwrap() = Some(SelectState { lens: lens.label.clone(), shot, url });
    // Glasses taken off (tray) while we were capturing: their cancel may have run too early.
    if !st(app).glasses_on.load(std::sync::atomic::Ordering::SeqCst) {
        finish_select(app, "", None);
    }
    Ok(())
}

pub fn is_selecting(app: &AppHandle) -> bool {
    st(app).capture.select.lock().unwrap().is_some()
}

/// Label of the lens currently in SELECT.
pub fn selecting_lens(app: &AppHandle) -> Option<String> {
    st(app).capture.select.lock().unwrap().as_ref().map(|s| s.lens.clone())
}

/// End SELECT (always, also on cancel): restore click-through, `hotkeys::set_escape(false)`,
/// emit `og://freeze null`. If `sel` is Some((rect, viewW, viewH)) from the selecting lens and
/// the rect is ≥ 8×8 CSS px, crop from the cached shot and call `actions::on_selection`.
/// Calls from a different lens label than the selecting one are ignored; an empty
/// `caller_lens` is a Rust-internal cancel and matches whichever lens is selecting.
/// HUD mode goes back to STANDBY (a started session then sets its own mode).
pub fn finish_select(app: &AppHandle, caller_lens: &str, sel: Option<(Rect, f64, f64)>) {
    let taken = {
        let state = st(app);
        let mut guard = state.capture.select.lock().unwrap();
        match guard.as_ref() {
            Some(s) if caller_lens.is_empty() || s.lens == caller_lens => guard.take(),
            _ => None,
        }
    };
    let Some(SelectState { lens, shot, url }) = taken else { return };
    windows::set_lens_interactive(app, &lens, false);
    hotkeys::set_escape(app, false);
    windows::emit_to_window(app, &lens, ev::FREEZE, None::<FreezePayload>);
    drop_frame(app, &url);
    telemetry::set_mode(app, HudMode::Standby);

    let Some((r, view_w, view_h)) = sel else { return };
    if r.w.abs() < MIN_SELECT_CSS || r.h.abs() < MIN_SELECT_CSS || view_w <= 0.0 || view_h <= 0.0 {
        return;
    }
    let (img_w, img_h) = shot.img.dimensions();
    let to_px = |css: f64, img: u32, view: f64| (css * img as f64 / view).round().clamp(0.0, img as f64) as i32;
    let (x0, x1) = (to_px(r.x.min(r.x + r.w), img_w, view_w), to_px(r.x.max(r.x + r.w), img_w, view_w));
    let (y0, y1) = (to_px(r.y.min(r.y + r.h), img_h, view_h), to_px(r.y.max(r.y + r.h), img_h, view_h));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let rect = PhysRect { x: shot.bounds.x + x0, y: shot.bounds.y + y0, w: x1 - x0, h: y1 - y0 };
    let Some(info) = windows::lens_by_label(app, &lens) else { return };
    // Callers include an async command and the 60 Hz cursor loop: keep encoding/requests off them.
    let app = app.clone();
    std::thread::spawn(move || actions::on_selection(&app, info, shot, rect));
}

/// Crop a physical sub-rect out of a shot.
pub fn crop(shot: &Shot, r: PhysRect) -> Option<image::RgbaImage> {
    let r = r.intersect(&shot.bounds)?;
    let (x, y) = ((r.x - shot.bounds.x) as u32, (r.y - shot.bounds.y) as u32);
    Some(image::imageops::crop_imm(&shot.img, x, y, r.w as u32, r.h as u32).to_image())
}

/// Every pixel exactly (0,0,0) (DRM / protected content).
pub fn is_all_black(img: &image::RgbaImage) -> bool {
    img.pixels().all(|p| p.0[..3] == [0, 0, 0])
}

/// Model upload policy: long edge ≤ 1568 px; PNG if ≤ 1.5 MP else JPEG q85; base64.
pub fn encode_for_model(img: &image::RgbaImage, screen: PhysRect) -> Result<ImagePayload, String> {
    let (w, h) = img.dimensions();
    let long = w.max(h);
    let scaled;
    let img = if long > MODEL_LONG_EDGE {
        let s = MODEL_LONG_EDGE as f64 / long as f64;
        let (tw, th) = ((w as f64 * s).round() as u32, (h as f64 * s).round() as u32);
        scaled = resize(img, tw, th, fr::FilterType::CatmullRom)?;
        &scaled
    } else {
        img
    };
    let (w, h) = img.dimensions();
    let (mime, bytes) = if w * h <= MODEL_PNG_MAX_PX {
        // RGB only: GDI captures may carry alpha = 0, which would make the PNG transparent.
        let rgb: Vec<u8> = img.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let mut out = Vec::new();
        PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
            .write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
        ("image/png", out)
    } else {
        ("image/jpeg", jpeg(img, 85)?)
    };
    let base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(ImagePayload { mime, base64, w, h, screen })
}

/// Downscale with fast_image_resize. Alpha is ignored (GDI alpha is not meaningful).
fn resize(src: &RgbaImage, w: u32, h: u32, filter: fr::FilterType) -> Result<RgbaImage, String> {
    let mut dst = RgbaImage::new(w.max(1), h.max(1));
    let opts = fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(filter)).use_alpha(false);
    fr::Resizer::new().resize(src, &mut dst, &opts).map_err(|e| e.to_string())?;
    Ok(dst)
}

/// JPEG-encode the RGB channels (alpha is dropped by the encoder).
fn jpeg(img: &RgbaImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(img).map_err(|e| e.to_string())?;
    Ok(out)
}
