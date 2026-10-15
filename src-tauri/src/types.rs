//! Mirror of `src/shared/ipc.ts` (authoritative) plus Rust-internal shared types.
//! Serde rules (DESIGN §3.8): structs are camelCase, tagged enums also rename
//! their fields, optional values are `Option<T>` <-> `T | null` (never skipped).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------- geometry

/// CSS px of a lens / monitor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Physical virtual-desktop pixels (GetCursorPos / UIA / xcap space). Rust-internal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhysRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PhysRect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn intersect(&self, o: &PhysRect) -> Option<PhysRect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        (x1 > x0 && y1 > y0).then(|| PhysRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }
}

// ---------------------------------------------------------------- enums

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    Lockon,
    Rail,
    Caption,
    Card,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Image,
    Scan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HotkeyAction {
    Explain,
    Capture,
    Scan,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Openai,
    Anthropic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BboxFormat {
    Pixel,
    Norm1000,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlassMode {
    Snapshot,
    Acrylic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelMode {
    Card,
    Rail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HudMode {
    Standby,
    Grabbing,
    Select,
    Thinking,
    Stream,
    Scan,
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub vision_model: Option<String>,
    /// JSON Merge Patch (RFC 7396) applied to every request body.
    pub extra_body: Option<serde_json::Map<String, serde_json::Value>>,
    pub bbox_format: BboxFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudCorners {
    pub tl: bool,
    pub tr: bool,
    pub bl: bool,
    pub br: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub providers: Vec<Provider>,
    pub active_provider_id: String,
    pub hotkeys: BTreeMap<HotkeyAction, String>,
    pub style: Style,
    pub hud_corners: HudCorners,
    pub lens_on: bool,
    pub glass: GlassMode,
    pub answer_language: String,
    pub include_window_title: bool,
    pub proxy: Option<String>,
    pub max_tokens: u32,
    pub exclude_from_capture: bool,
}

// ---------------------------------------------------------------- HUD

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStat {
    pub name: String,
    pub util: f64,
    pub vram_used_gb: f64,
    pub vram_total_gb: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HudState {
    pub mode: HudMode,
    pub app: Option<String>,
    pub cpu: f64,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub gpu: Option<GpuStat>,
    pub provider: String,
    pub model: String,
    pub ttft_ms: Option<u64>,
    pub tps: Option<f64>,
    pub tokens_today: u64,
}

impl Default for HudState {
    fn default() -> Self {
        HudState {
            mode: HudMode::Standby,
            app: None,
            cpu: 0.0,
            ram_used_gb: 0.0,
            ram_total_gb: 0.0,
            gpu: None,
            provider: String::new(),
            model: String::new(),
            ttft_ms: None,
            tps: None,
            tokens_today: 0,
        }
    }
}

// ---------------------------------------------------------------- sessions

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Glass {
    pub url: String,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanItem {
    pub key: String,
    pub label: String,
    pub brief: String,
    pub detail: String,
    pub rect: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

/// Frontend view of the active session (also returned in `InitState.active`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub kind: Kind,
    pub style: Style,
    pub lens: String,
    pub anchor: Option<Rect>,
    pub cursor: Point,
    pub glass: Option<Glass>,
    pub messages: Vec<Message>,
    pub streaming: bool,
    pub error: Option<String>,
    pub items: Option<Vec<ScanItem>>,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SessionEvent {
    Start {
        id: String,
        kind: Kind,
        style: Style,
        lens: String,
        anchor: Option<Rect>,
        cursor: Point,
        glass: Option<Glass>,
        preview: String,
    },
    User {
        id: String,
        msg_index: usize,
        text: String,
    },
    Delta {
        id: String,
        msg_index: usize,
        text: String,
    },
    Done {
        id: String,
        msg_index: usize,
    },
    Error {
        id: String,
        msg_index: usize,
        code: String,
        message: String,
    },
    Scan {
        id: String,
        items: Vec<ScanItem>,
    },
    Stale {
        id: String,
    },
    Dismiss {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelEvent {
    pub id: String,
    pub mode: PanelMode,
    pub r#box: Rect,
    pub grow_up: bool,
    pub item_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub kind: Kind,
    pub tldr: String,
    pub text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitState {
    pub label: String,
    pub is_primary: bool,
    pub work_area: Option<Rect>,
    pub settings: Settings,
    pub hud: Option<HudState>,
    pub active: Option<Session>,
    pub panel: Option<PanelEvent>,
    pub glasses_on: bool,
}

// ---------------------------------------------------------------- small event payloads

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPayload {
    pub x: f64,
    pub y: f64,
    pub in_panel: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToastEvent {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LensOnEvent {
    pub frames: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezePayload {
    pub url: String,
    pub img_w: u32,
    pub img_h: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VisibilityEvent {
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsResult {
    pub hotkey_errors: BTreeMap<HotkeyAction, String>,
}

// ---------------------------------------------------------------- event names

pub mod ev {
    pub const CURSOR: &str = "og://cursor";
    pub const HUD: &str = "og://hud";
    pub const TOAST: &str = "og://toast";
    pub const LENS_ON: &str = "og://lens-on";
    pub const FREEZE: &str = "og://freeze";
    pub const SESSION: &str = "og://session";
    pub const PANEL: &str = "og://panel";
    pub const SETTINGS: &str = "og://settings";
    pub const VISIBILITY: &str = "og://visibility";
}

// ---------------------------------------------------------------- Rust-internal shared types

/// One lens window per monitor (DESIGN §3.1). Owned by windows.rs.
#[derive(Debug, Clone, PartialEq)]
pub struct LensInfo {
    /// `lens-<n>`, n from a never-reused counter.
    pub label: String,
    /// Monitor device name, e.g. `\\.\DISPLAY1` (never used as a label).
    pub monitor: String,
    /// Full monitor rect, physical px.
    pub bounds: PhysRect,
    /// Work area (monitor minus taskbar), physical px.
    pub work_area: PhysRect,
    pub scale: f64,
    pub primary: bool,
    /// `frontend_ready` received; never emit to / show the window before this.
    pub ready: bool,
    /// Hidden because a fullscreen app is in the foreground on this monitor.
    pub suppressed: bool,
}

impl LensInfo {
    pub fn to_css_point(&self, x: i32, y: i32) -> Point {
        Point {
            x: (x - self.bounds.x) as f64 / self.scale,
            y: (y - self.bounds.y) as f64 / self.scale,
        }
    }
    pub fn to_css(&self, r: PhysRect) -> Rect {
        let p = self.to_css_point(r.x, r.y);
        Rect { x: p.x, y: p.y, w: r.w as f64 / self.scale, h: r.h as f64 / self.scale }
    }
    pub fn to_phys(&self, r: Rect) -> PhysRect {
        PhysRect {
            x: self.bounds.x + (r.x * self.scale).round() as i32,
            y: self.bounds.y + (r.y * self.scale).round() as i32,
            w: (r.w * self.scale).round() as i32,
            h: (r.h * self.scale).round() as i32,
        }
    }
    pub fn css_size(&self) -> (f64, f64) {
        (self.bounds.w as f64 / self.scale, self.bounds.h as f64 / self.scale)
    }
}

/// Panel bookkeeping. Owned by windows.rs.
#[derive(Debug, Clone, Default)]
pub struct PanelInfo {
    pub ready: bool,
    pub visible: bool,
    /// Current request; None when hidden.
    pub event: Option<PanelEvent>,
    /// Lens label whose CSS space `event.box` is expressed in.
    pub lens: Option<String>,
    /// Physical rect of the panel while visible.
    pub phys: Option<PhysRect>,
}

/// The window the user was working in when a trigger fired. Produced by grab.rs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceInfo {
    /// HWND as isize (0 = none).
    pub hwnd: isize,
    pub process: String,
    pub title: String,
    /// Window rect (DWM extended frame bounds), physical px.
    pub rect: PhysRect,
    pub minimized: bool,
}

/// An image ready to send to a VLM. Produced by capture.rs.
#[derive(Debug, Clone, PartialEq)]
pub struct ImagePayload {
    pub mime: &'static str,
    pub base64: String,
    /// Size of the image actually sent.
    pub w: u32,
    pub h: u32,
    /// Physical rect on screen that the image covers (for mapping boxes back).
    pub screen: PhysRect,
}
