//! Runtime theme packs (DESIGN §10): discovery in the built-in and user theme folders, the
//! `ogtheme://` protocol, and 1 s fingerprint polling for hot reload. CSS + static assets only.

use crate::state::st;
use crate::types::*;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};
use tauri::http::{header, Request, Response};
use tauri::{AppHandle, Emitter, Manager, UriSchemeContext, UriSchemeResponder, Wry};
use tauri_plugin_opener::OpenerExt;

const BASE: &str = "http://ogtheme.localhost/";
/// Built-ins come first, in this order; other built-ins follow by id, then user-only themes.
const BUILTIN_ORDER: [&str; 3] = ["terminal", "holo", "neon"];
const MAX_FILE: u64 = 8 << 20;

#[derive(Default)]
pub struct ThemeState {
    /// Last scan result + revision (bumped on every reload).
    pub current: Mutex<Option<ThemesEvent>>,
}

/// `theme.json`. Missing fields take defaults; a bad `bootMs` falls back to 1000.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Manifest {
    name: String,
    author: String,
    version: String,
    description: String,
    #[serde(rename = "bootMs")]
    boot_ms: serde_json::Value,
}

/// Built-in themes: `<resource_dir>/themes`. User themes: `<app_config_dir>/themes` (created
/// on demand). A user theme overrides a built-in one with the same id.
pub fn user_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map(|d| d.join("themes")).map_err(|e| e.to_string())
}

/// tauri-build copies `bundle.resources` next to the exe at build time, so in debug builds
/// prefer the repo folder: editing a built-in theme then hot-reloads without a rebuild.
fn builtin_dir(app: &AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../themes");
        if repo.is_dir() {
            return Some(repo);
        }
    }
    app.path().resource_dir().ok().map(|d| d.join("themes"))
}

/// Current themes (scans on first use).
pub fn list(app: &AppHandle) -> ThemesEvent {
    cur(app).get_or_insert_with(|| ThemesEvent { themes: scan(app, 0), rev: 0 }).clone()
}

/// Force a rescan, then broadcast `og://themes`. Always bumps `rev` (callers are the watcher on
/// a file change and the Reload button), so every window refetches CSS whose content changed.
pub fn reload(app: &AppHandle) -> ThemesEvent {
    let ev = {
        let mut cur = cur(app);
        let rev = cur.as_ref().map_or(0, |e| e.rev + 1);
        let ev = ThemesEvent { themes: scan(app, rev), rev };
        *cur = Some(ev.clone());
        ev
    };
    log::info!("themes: rev {} ({} found)", ev.rev, ev.themes.len());
    if let Err(e) = app.emit(ev::THEMES, &ev) {
        log::warn!("emit themes: {e}");
    }
    ev
}

/// Poll both theme folders every second (path, size, mtime fingerprint); on change -> `reload`.
pub fn spawn_watcher(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("og-themes".into()).spawn(move || {
        let mut last = fingerprint(&app);
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let fp = fingerprint(&app);
            if fp != last {
                last = fp;
                reload(&app);
            }
        }
    });
    if let Err(e) = spawned {
        log::error!("themes thread: {e}");
    }
}

/// `ogtheme://<id>/<relative path>`: serve a file from that theme's folder only (no `..`,
/// no backslashes, extension whitelist, ≤ 8 MB) with Content-Type, `Cache-Control: no-store`
/// and `Access-Control-Allow-Origin: *` (mask-image / @font-face fetch in CORS mode); else 404.
/// WebView2 calls this on the main thread, so the disk read happens on a worker thread.
pub fn serve(ctx: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().to_string();
    std::thread::spawn(move || {
        let res = Response::builder()
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
        let res = match load(&app, &path) {
            Some((mime, bytes)) => res.header(header::CONTENT_TYPE, mime).body(bytes),
            None => res.status(404).body(Vec::new()),
        };
        responder.respond(res.unwrap_or_default());
    });
}

/// Open the user theme folder in Explorer (create it first).
pub fn open_folder(app: &AppHandle) -> Result<(), String> {
    let dir = user_dir(app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- scanning

fn cur(app: &AppHandle) -> MutexGuard<'_, Option<ThemesEvent>> {
    st(app).inner().themes.current.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `^[a-z0-9][a-z0-9-]{0,31}$`
fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    (1..=32).contains(&b.len())
        && b[0] != b'-'
        && b.iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn scan(app: &AppHandle, rev: u64) -> Vec<ThemeInfo> {
    let mut themes = builtin_dir(app).map_or_else(Vec::new, |d| scan_dir(&d, true, rev));
    // Stable sort: within the "other" rank the id order from scan_dir is kept.
    themes.sort_by_key(|t| BUILTIN_ORDER.iter().position(|&b| b == t.id).unwrap_or(BUILTIN_ORDER.len()));
    if let Ok(dir) = user_dir(app) {
        for t in scan_dir(&dir, false, rev) {
            match themes.iter_mut().find(|b| b.id == t.id) {
                Some(slot) => *slot = t,
                None => themes.push(t),
            }
        }
    }
    themes
}

/// Valid theme folders directly under `dir`, sorted by id. Broken ones are logged and skipped.
fn scan_dir(dir: &Path, builtin: bool, rev: u64) -> Vec<ThemeInfo> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<ThemeInfo> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            let res = if valid_id(&id) { read_theme(&e.path()) } else { Err("invalid folder name".into()) };
            match res {
                Ok(m) => Some(info(id, m, builtin, rev)),
                Err(err) => {
                    log::warn!("theme {}: {err}", e.path().display());
                    None
                }
            }
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn read_theme(dir: &Path) -> Result<Manifest, String> {
    if !dir.join("theme.css").is_file() {
        return Err("missing theme.css".into());
    }
    let text = fs::read_to_string(dir.join("theme.json")).map_err(|e| format!("theme.json: {e}"))?;
    // Notepad saves UTF-8 with a BOM, which serde_json rejects.
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("theme.json: {e}"))
}

fn info(id: String, m: Manifest, builtin: bool, rev: u64) -> ThemeInfo {
    ThemeInfo {
        name: if m.name.trim().is_empty() { id.clone() } else { m.name },
        author: m.author,
        version: m.version,
        description: m.description,
        boot_ms: m.boot_ms.as_f64().map_or(1000, |n| n.clamp(300.0, 4000.0).round() as u32),
        builtin,
        // The revision is a path segment, not a query, so relative url()s in the sheet
        // (svg/png/fonts) resolve to new URLs too and hot-reload with it.
        css_url: format!("{BASE}{id}/r{rev}/theme.css"),
        id,
    }
}

/// Every file under both theme roots: (path, size, mtime), sorted. Cheap for a few themes.
fn fingerprint(app: &AppHandle) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        // Skip dot entries (.git in a theme author's folder would cost thousands of stats/s).
        for e in entries.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')) {
            let path = e.path();
            // fs::metadata follows symlinks (a linked dev folder is watched too); the depth
            // cap stops symlink loops.
            let Ok(m) = fs::metadata(&path) else { continue };
            if m.is_dir() {
                if depth < 4 {
                    walk(&path, depth + 1, out);
                }
            } else {
                out.push((path, m.len(), m.modified().ok()));
            }
        }
    }
    let mut out = Vec::new();
    for root in [builtin_dir(app), user_dir(app).ok()].into_iter().flatten() {
        walk(&root, 0, &mut out);
    }
    out.sort();
    out
}

// ---------------------------------------------------------------- protocol

/// Resolve `/<id>/<rest>` to (mime, bytes); None for anything outside the rules.
fn load(app: &AppHandle, raw: &str) -> Option<(&'static str, Vec<u8>)> {
    let path = percent_decode(raw.trim_start_matches('/'))?;
    let (id, rest) = path.split_once('/')?;
    // Drop the cache-busting `r<rev>/` segment that css_url puts in front of every theme file.
    let rest = match rest.split_once('/') {
        Some((seg, tail)) if seg.len() > 1 && seg.starts_with('r') && seg[1..].bytes().all(|c| c.is_ascii_digit()) => tail,
        _ => rest,
    };
    // ':' covers drive letters and NTFS alternate streams; an empty segment covers absolute
    // paths and `//`.
    if !valid_id(id) || rest.contains(['\\', ':', '\0']) {
        return None;
    }
    if rest.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return None;
    }
    let mime = mime(&rest.rsplit_once('.')?.1.to_ascii_lowercase())?;
    let builtin = list(app).themes.iter().find(|t| t.id == id)?.builtin;
    let root = if builtin { builtin_dir(app)? } else { user_dir(app).ok()? };
    let root = root.join(id).canonicalize().ok()?;
    let file = root.join(rest).canonicalize().ok()?;
    let meta = fs::metadata(&file).ok()?;
    if !file.starts_with(&root) || !meta.is_file() || meta.len() > MAX_FILE {
        return None;
    }
    let bytes = fs::read(&file).ok()?;
    (bytes.len() as u64 <= MAX_FILE).then_some((mime, bytes))
}

fn mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "json" => "application/json",
        _ => return None,
    })
}

/// `%XX` -> byte; the result must be UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = b.get(i + 1..i + 3).filter(|h| h.iter().all(u8::is_ascii_hexdigit))?;
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
