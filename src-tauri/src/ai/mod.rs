//! Sessions and LLM requests (DESIGN §2.2, §3.5). One active session + read-only history (≤10).
//! The only writer of `og://session` (to the session's lens + the panel).

mod anthropic;
mod openai;
mod prompts;

use crate::types::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::AppHandle;

/// Internal per-session data (never sent to the frontend as-is).
pub struct Active {
    pub view: Session,
    pub source: Option<SourceInfo>,
    /// The original source text (for the Alt+Q toggle rule).
    pub source_text: String,
    /// Image of the first user turn (resent on every follow-up).
    pub image: Option<ImagePayload>,
    /// Provider frozen at start.
    pub provider: Provider,
    /// Per assistant message: raw Anthropic content blocks to replay verbatim (thinking incl.).
    pub raw_assistant: Vec<Option<serde_json::Value>>,
    pub stale: bool,
    pub cancel: tokio_util::sync::CancellationToken,
}

#[derive(Default)]
pub struct AiState {
    pub active: Mutex<Option<Active>>,
    pub history: Mutex<VecDeque<HistoryEntry>>,
}

pub struct NewSession {
    pub kind: Kind,
    pub lens: LensInfo,
    pub anchor: Option<Rect>,
    pub cursor: Point,
    pub glass: Option<Glass>,
    /// Text shown as messages[0] (selection text, or a short caption for image/scan).
    pub preview: String,
    /// Raw selected text (Kind::Text) for the toggle rule; empty otherwise.
    pub source_text: String,
    pub image: Option<ImagePayload>,
    pub source: Option<SourceInfo>,
}

/// Summary of the active session for actions.rs / cursor.rs decisions.
#[derive(Debug, Clone)]
pub struct ActiveMeta {
    pub id: String,
    pub kind: Kind,
    pub style: Style,
    pub lens: String,
    pub pinned: bool,
    pub errored: bool,
    pub stale: bool,
    pub source_text: String,
    pub source: Option<SourceInfo>,
}

/// Start a session with the current settings' style + provider: emit `start` (dismissing any
/// previous session first), set HUD mode, spawn the request. Returns the session id.
pub fn start(app: &AppHandle, ns: NewSession) -> String {
    todo!()
}

/// Follow-up in the panel: append user + empty assistant, emit `user`, stream into k+1.
/// Rejected while streaming. Pins the session.
pub fn followup(app: &AppHandle, id: &str, text: String) -> Result<(), String> {
    todo!()
}

/// Dismiss if `id` is the active session: cancel, emit `dismiss`, push to history, hide the
/// panel (restoring focus to the source window). Returns true if something was dismissed.
pub fn dismiss(app: &AppHandle, id: &str) -> bool {
    todo!()
}

/// Dismiss whatever is active (no-op if none).
pub fn dismiss_active(app: &AppHandle) -> bool {
    todo!()
}

pub fn pin(app: &AppHandle, id: &str, pinned: bool) {
    todo!()
}

/// Mark the active session stale (emit `stale` once): brackets/leader lines hide.
pub fn mark_stale(app: &AppHandle) {
    todo!()
}

pub fn active_meta(app: &AppHandle) -> Option<ActiveMeta> {
    todo!()
}

/// Frontend view of the active session (for InitState).
pub fn snapshot(app: &AppHandle) -> Option<Session> {
    todo!()
}

pub fn history(app: &AppHandle) -> Vec<HistoryEntry> {
    todo!()
}

/// Minimal non-streaming-equivalent request ("Reply with OK") using the given draft provider
/// (key from keyring). Ok(model reply snippet) or Err(error code + message).
pub async fn test_provider(app: &AppHandle, p: Provider) -> Result<String, String> {
    todo!()
}
