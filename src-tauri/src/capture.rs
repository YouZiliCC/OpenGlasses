//! Screen capture, the `ogframe://` protocol, snapshot glass, Lens-On frames, SELECT,
//! and image encoding for models (DESIGN §2.4, §3.4).

use crate::types::*;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::http::{Request, Response};
use tauri::{AppHandle, UriSchemeContext, UriSchemeResponder, Wry};

/// A full-monitor capture held in memory.
pub struct Shot {
    pub img: image::RgbaImage,
    /// Physical rect the image covers (the monitor bounds).
    pub bounds: PhysRect,
}

pub struct SelectState {
    pub lens: String,
    pub shot: Shot,
}

#[derive(Default)]
pub struct CaptureState {
    /// key (`<uuid>.jpg|png`) -> (mime, bytes) served by `ogframe://`.
    pub frames: Mutex<HashMap<String, (&'static str, Vec<u8>)>>,
    pub select: Mutex<Option<SelectState>>,
}

/// `ogframe://` handler: serve `frames[key]` with Content-Type and `Cache-Control: no-store`; 404 otherwise.
pub fn serve(ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let _: Option<Response<Vec<u8>>> = None;
    todo!()
}

/// Store bytes and return the full URL `http://ogframe.localhost/<key>`.
pub fn put_frame(app: &AppHandle, mime: &'static str, bytes: Vec<u8>) -> String {
    todo!()
}

/// Remove a frame by URL or key (no-op if missing).
pub fn drop_frame(app: &AppHandle, url: &str) {
    todo!()
}

/// Capture one monitor with xcap (GDI) on the calling worker thread, excluding our overlays
/// (`windows::without_overlays_in_capture`).
pub fn grab_monitor(app: &AppHandle, lens: &LensInfo) -> Result<Shot, String> {
    todo!()
}

/// Snapshot glass from a shot: downscale to 1/8, darken/tint, JPEG q70, `put_frame`.
/// Returned w/h = the lens CSS size.
pub fn glass_from(app: &AppHandle, shot: &Shot, lens: &LensInfo) -> Option<Glass> {
    todo!()
}

/// Lens-On on the primary monitor: capture, 1/32 1/12 1/4 JPEG frames, emit `og://lens-on`
/// with `frames: null` if over the 150 ms budget or on failure. Frames dropped after 5 s.
/// Runs on a worker thread.
pub fn play_lens_on(app: &AppHandle) {
    todo!()
}

/// Enter SELECT on `lens`: capture, emit `og://freeze {url,imgW,imgH}` (JPEG q85 full size),
/// hide the panel, `windows::set_lens_interactive(on)`, `hotkeys::set_escape(true)`.
pub fn begin_select(app: &AppHandle, lens: &LensInfo) -> Result<(), String> {
    todo!()
}

pub fn is_selecting(app: &AppHandle) -> bool {
    todo!()
}

/// Label of the lens currently in SELECT.
pub fn selecting_lens(app: &AppHandle) -> Option<String> {
    todo!()
}

/// End SELECT (always, also on cancel): restore click-through, `hotkeys::set_escape(false)`,
/// emit `og://freeze null`. If `sel` is Some((rect, viewW, viewH)) from the selecting lens and
/// the rect is ≥ 8×8 CSS px, crop from the cached shot and call `actions::on_selection`.
/// Calls from a different lens label than the selecting one are ignored.
pub fn finish_select(app: &AppHandle, caller_lens: &str, sel: Option<(Rect, f64, f64)>) {
    todo!()
}

/// Crop a physical sub-rect out of a shot.
pub fn crop(shot: &Shot, r: PhysRect) -> Option<image::RgbaImage> {
    todo!()
}

/// Every pixel exactly (0,0,0) (DRM / protected content).
pub fn is_all_black(img: &image::RgbaImage) -> bool {
    todo!()
}

/// Model upload policy: long edge ≤ 1568 px; PNG if ≤ 1.5 MP else JPEG q85; base64.
pub fn encode_for_model(img: &image::RgbaImage, screen: PhysRect) -> Result<ImagePayload, String> {
    todo!()
}
