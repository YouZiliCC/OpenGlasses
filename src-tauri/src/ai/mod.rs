//! Sessions and LLM requests (DESIGN §2.2, §3.5). One active session + read-only history (≤10).
//! The only writer of `og://session` (to the session's lens + the panel).

mod anthropic;
mod openai;
mod prompts;

use crate::state::{self, st};
use crate::types::*;
use crate::{actions, capture, settings, telemetry, windows};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{Map, Value};
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

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
    /// Frozen at start so every turn resends an identical prefix.
    system: String,
    /// What the model actually got as the first user turn (messages[0] is only the preview).
    first_user: String,
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

const BATCH: Duration = Duration::from_millis(40);
const IDLE: Duration = Duration::from_secs(30);
const HISTORY_MAX: usize = 10;

/// Start a session with the current settings' style + provider: emit `start` (dismissing any
/// previous session first), set HUD mode, spawn the request. Returns the session id.
pub fn start(app: &AppHandle, ns: NewSession) -> String {
    dismiss_active(app);
    let s = state::settings(app);
    let provider = settings::active_provider(&s);
    let id = uuid::Uuid::new_v4().to_string();
    let text = if ns.source_text.is_empty() { &ns.preview } else { &ns.source_text };
    let first_user = prompts::first_user(
        ns.kind,
        text,
        ns.source.as_ref(),
        s.include_window_title,
        ns.image.as_ref(),
        provider.bbox_format,
    );
    let view = Session {
        id: id.clone(),
        kind: ns.kind,
        style: s.style,
        lens: ns.lens.label.clone(),
        anchor: ns.anchor,
        cursor: ns.cursor,
        glass: ns.glass.clone(),
        messages: vec![msg(Role::User, &ns.preview), msg(Role::Assistant, "")],
        streaming: true,
        error: None,
        items: None,
        pinned: false,
    };
    let start = SessionEvent::Start {
        id: id.clone(),
        kind: ns.kind,
        style: s.style,
        lens: ns.lens.label.clone(),
        anchor: ns.anchor,
        cursor: ns.cursor,
        glass: ns.glass,
        preview: ns.preview,
    };
    let cancel = CancellationToken::new();
    let active = Active {
        view,
        source: ns.source,
        source_text: ns.source_text,
        image: ns.image,
        provider,
        raw_assistant: vec![None, None],
        stale: false,
        cancel: cancel.clone(),
        system: prompts::system(ns.kind, &s.answer_language),
        first_user,
    };
    // A concurrent start may have slipped in after dismiss_active: close it too.
    let old = lock(&ai(app).active).replace(active);
    if let Some(old) = old {
        close(app, old);
    }
    emit(app, &ns.lens.label, start);
    tauri::async_runtime::spawn(run(app.clone(), id.clone(), 1, cancel));
    id
}

/// Follow-up in the panel: append user + empty assistant, emit `user`, stream into k+1.
/// Rejected while streaming. Pins the session.
pub fn followup(app: &AppHandle, id: &str, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("EMPTY".into());
    }
    let (lens, k, cancel) = {
        let mut g = lock(&ai(app).active);
        let a = g.as_mut().filter(|a| a.view.id == id).ok_or("NO SESSION")?;
        if a.view.streaming {
            return Err("BUSY".into());
        }
        let k = a.view.messages.len();
        a.view.messages.push(msg(Role::User, &text));
        a.view.messages.push(msg(Role::Assistant, ""));
        a.raw_assistant.extend([None, None]);
        a.view.pinned = true;
        a.view.streaming = true;
        a.view.error = None;
        (a.view.lens.clone(), k, a.cancel.clone())
    };
    emit(app, &lens, SessionEvent::User { id: id.into(), msg_index: k, text });
    tauri::async_runtime::spawn(run(app.clone(), id.into(), k + 1, cancel));
    Ok(())
}

/// Dismiss if `id` is the active session: cancel, emit `dismiss`, push to history, hide the
/// panel (restoring focus to the source window). Returns true if something was dismissed.
pub fn dismiss(app: &AppHandle, id: &str) -> bool {
    let taken = {
        let mut g = lock(&ai(app).active);
        if g.as_ref().is_some_and(|a| a.view.id == id) { g.take() } else { None }
    };
    taken.map(|a| close(app, a)).is_some()
}

/// Dismiss whatever is active (no-op if none).
pub fn dismiss_active(app: &AppHandle) -> bool {
    let taken = lock(&ai(app).active).take();
    taken.map(|a| close(app, a)).is_some()
}

pub fn pin(app: &AppHandle, id: &str, pinned: bool) {
    if let Some(a) = lock(&ai(app).active).as_mut().filter(|a| a.view.id == id) {
        a.view.pinned = pinned;
    }
}

/// Mark the active session stale (emit `stale` once): brackets/leader lines hide.
pub fn mark_stale(app: &AppHandle) {
    let hit = {
        let mut g = lock(&ai(app).active);
        match g.as_mut().filter(|a| !a.stale) {
            Some(a) => {
                a.stale = true;
                Some((a.view.id.clone(), a.view.lens.clone()))
            }
            None => None,
        }
    };
    if let Some((id, lens)) = hit {
        emit(app, &lens, SessionEvent::Stale { id });
    }
}

pub fn active_meta(app: &AppHandle) -> Option<ActiveMeta> {
    lock(&ai(app).active).as_ref().map(|a| ActiveMeta {
        id: a.view.id.clone(),
        kind: a.view.kind,
        style: a.view.style,
        lens: a.view.lens.clone(),
        pinned: a.view.pinned,
        errored: a.view.error.is_some(),
        stale: a.stale,
        source_text: a.source_text.clone(),
        source: a.source.clone(),
    })
}

/// Frontend view of the active session (for InitState).
pub fn snapshot(app: &AppHandle) -> Option<Session> {
    lock(&ai(app).active).as_ref().map(|a| a.view.clone())
}

pub fn history(app: &AppHandle) -> Vec<HistoryEntry> {
    lock(&ai(app).history).iter().cloned().collect()
}

/// Minimal non-streaming-equivalent request ("Reply with OK") using the given draft provider
/// (key from keyring). Ok(model reply snippet) or Err(error code + message).
pub async fn test_provider(app: &AppHandle, p: Provider) -> Result<String, String> {
    let proxy = state::settings(app).proxy;
    let kind = p.kind;
    let fail = |e: AiError| format!("{}: {}", e.code, e.message);
    let req = Req {
        key: api_key(&p).map_err(fail)?,
        base_url: p.base_url,
        model: p.model,
        system: String::new(),
        turns: vec![Turn { role: Role::User, text: "Reply with exactly: OK".into(), raw: None }],
        image: None,
        max_tokens: 16,
        extra: p.extra_body,
    };
    let client = client(&req.base_url, proxy.as_deref()).map_err(fail)?;
    let builder = match kind {
        ProviderKind::Openai => openai::request(&client, &req, false),
        ProviderKind::Anthropic => anthropic::request(&client, &req, false),
    };
    let fut = async {
        let resp = builder.send().await.map_err(|e| net_error(&e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| net_error(&e))?;
        if !status.is_success() {
            return Err(http_error(status, &body));
        }
        let v: Value = serde_json::from_str(&body)
            .map_err(|_| AiError::new("HTTP 200", clip(body.trim(), 200)))?;
        match kind {
            ProviderKind::Openai => openai::reply_text(&v),
            ProviderKind::Anthropic => anthropic::reply_text(&v),
        }
    };
    let text = tokio::time::timeout(first_byte_timeout(&req.base_url), fut)
        .await
        .map_err(|_| AiError::new("TIMEOUT", "no response"))
        .and_then(|r| r)
        .map_err(fail)?;
    let text = text.trim();
    Ok(if text.is_empty() { "(connected, empty reply)".into() } else { clip(text, 80) })
}

// ---------------------------------------------------------------- shared with the protocol files

/// A request, protocol-neutral. `turns[0]` is the first user turn; `image` belongs to it.
pub(super) struct Req {
    pub base_url: String,
    pub model: String,
    pub key: Option<String>,
    pub system: String,
    pub turns: Vec<Turn>,
    pub image: Option<ImagePayload>,
    pub max_tokens: u32,
    pub extra: Option<Map<String, Value>>,
}

pub(super) struct Turn {
    pub role: Role,
    pub text: String,
    /// Anthropic content blocks to replay instead of `text`.
    pub raw: Option<Value>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
    #[default]
    Normal,
    Length,
    Refused,
}

/// What the protocol parsers extracted from the stream so far.
#[derive(Default)]
pub(super) struct Parsed {
    /// Terminal event seen (`[DONE]` / `message_stop`).
    pub done: bool,
    /// A finish/stop reason was seen (enough to accept an EOF without the terminal event).
    pub finished: bool,
    pub stop: Stop,
    /// Refusal category, if the provider gave one.
    pub detail: Option<String>,
    pub output_tokens: Option<u64>,
    /// Anthropic content blocks as streamed.
    pub blocks: Vec<Value>,
}

pub(super) struct AiError {
    pub code: String,
    pub message: String,
}

impl AiError {
    pub(super) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        AiError { code: code.into(), message: message.into() }
    }
}

/// JSON Merge Patch (RFC 7396): `null` deletes, objects merge recursively, the rest replaces.
pub(super) fn merge_patch(target: &mut Value, patch: &Map<String, Value>) {
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let t = target.as_object_mut().expect("object");
    for (k, v) in patch {
        match v {
            Value::Null => {
                t.remove(k);
            }
            Value::Object(p) => merge_patch(t.entry(k.clone()).or_insert(Value::Null), p),
            _ => {
                t.insert(k.clone(), v.clone());
            }
        }
    }
}

/// Error object sent inside a stream (both protocols use `{type?, code?, message}`).
pub(super) fn stream_error(e: &Value) -> AiError {
    let code = match (e["type"].as_str(), e["code"].as_u64()) {
        (Some("rate_limit_error"), _) => "RATE LIMIT".into(),
        (Some("overloaded_error"), _) => "HTTP 529".into(),
        (_, Some(c)) => status_code(c as u16),
        _ => "HTTP 500".into(),
    };
    let message = e["message"].as_str().or(e.as_str()).unwrap_or("stream error");
    AiError::new(code, clip(message, 200))
}

/// Lowercase host of a base URL ("" if unparseable).
pub(super) fn host(base_url: &str) -> String {
    reqwest::Url::parse(base_url.trim())
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_matches(['[', ']']).to_ascii_lowercase()))
        .unwrap_or_default()
}

// ---------------------------------------------------------------- session internals

fn ai(app: &AppHandle) -> &AiState {
    &st(app).inner().ai
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn msg(role: Role, text: &str) -> Message {
    Message { role, text: text.into() }
}

fn emit(app: &AppHandle, lens: &str, ev: SessionEvent) {
    windows::emit_to_window(app, lens, ev::SESSION, ev.clone());
    windows::emit_to_panel(app, ev::SESSION, ev);
}

/// Run `f` on the active session if it is still `id` and not cancelled; also returns its lens.
fn with_active<R>(app: &AppHandle, id: &str, f: impl FnOnce(&mut Active) -> R) -> Option<(String, R)> {
    let mut g = lock(&ai(app).active);
    let a = g.as_mut().filter(|a| a.view.id == id && !a.cancel.is_cancelled())?;
    let r = f(a);
    Some((a.view.lens.clone(), r))
}

/// Tear down a session that was already removed from `AiState.active`.
fn close(app: &AppHandle, a: Active) {
    a.cancel.cancel();
    emit(app, &a.view.lens, SessionEvent::Dismiss { id: a.view.id.clone() });
    if let Some(e) = history_entry(&a.view) {
        let mut h = lock(&ai(app).history);
        h.push_front(e);
        h.truncate(HISTORY_MAX);
    }
    if let Some(g) = &a.view.glass {
        capture::drop_frame(app, &g.url);
    }
    windows::hide_panel(app, a.source.as_ref().map_or(0, |s| s.hwnd));
    telemetry::set_mode(app, HudMode::Standby);
}

fn history_entry(v: &Session) -> Option<HistoryEntry> {
    let text = match &v.items {
        // Scan answers are raw JSON: keep a readable list instead.
        Some(items) => items.iter().map(|i| format!("- **{}** — {}", i.label, i.brief)).collect::<Vec<_>>().join("\n"),
        None => v.messages.get(1)?.text.clone(),
    };
    if text.trim().is_empty() {
        return None;
    }
    let created_at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    Some(HistoryEntry { id: v.id.clone(), kind: v.kind, tldr: tldr(&text), text, created_at })
}

/// First non-empty prose line, Markdown stripped, ≤120 chars (cf. src/shared/tldr.ts).
fn tldr(text: &str) -> String {
    let mut in_block = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("```") || line.starts_with("$$") {
            if !(line.starts_with("$$") && line.len() > 2 && line.ends_with("$$")) {
                in_block = !in_block;
            }
            continue;
        }
        if in_block {
            continue;
        }
        let l = line.trim_start_matches('#').trim_start();
        let l = l.strip_prefix(['-', '*', '+', '>']).filter(|r| r.starts_with(' ')).unwrap_or(l);
        let plain = l.replace("**", "").replace("__", "").replace('`', "");
        let plain = plain.trim();
        if !plain.is_empty() {
            return clip(plain, 120);
        }
    }
    String::new()
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.truncate(out.trim_end().len());
    out + "…"
}

// ---------------------------------------------------------------- requests

fn is_local(base_url: &str) -> bool {
    matches!(host(base_url).as_str(), "localhost" | "127.0.0.1" | "::1")
}

fn first_byte_timeout(base_url: &str) -> Duration {
    Duration::from_secs(if is_local(base_url) { 90 } else { 30 })
}

fn api_key(p: &Provider) -> Result<Option<String>, AiError> {
    let key = settings::get_api_key(&p.id).filter(|k| !k.trim().is_empty());
    if key.is_none() && !is_local(&p.base_url) {
        return Err(AiError::new("NO API KEY", format!("No API key saved for {}", p.name)));
    }
    Ok(key)
}

fn client(base_url: &str, proxy: Option<&str>) -> Result<reqwest::Client, AiError> {
    let mut b = reqwest::Client::builder().user_agent(concat!("OpenGlasses/", env!("CARGO_PKG_VERSION")));
    if is_local(base_url) {
        b = b.no_proxy();
    } else if let Some(p) = proxy.map(str::trim).filter(|p| !p.is_empty()) {
        let p = reqwest::Proxy::all(p).map_err(|e| AiError::new("NETWORK", format!("invalid proxy: {e}")))?;
        b = b.proxy(p);
    }
    b.build().map_err(|e| net_error(&e))
}

fn status_code(s: u16) -> String {
    match s {
        401 | 403 => format!("AUTH {s}"),
        429 => "RATE LIMIT".into(),
        _ => format!("HTTP {s}"),
    }
}

/// Non-2xx response: code from the status, message from the JSON error body when possible.
fn http_error(status: reqwest::StatusCode, body: &str) -> AiError {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let e = v.get("error").unwrap_or(&v);
    let message = e["message"]
        .as_str()
        .or(e.as_str())
        .or(v["message"].as_str())
        .or(v["detail"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let b = body.trim();
            if b.is_empty() || b.starts_with('<') { status.canonical_reason().unwrap_or("").into() } else { b.into() }
        });
    AiError::new(status_code(status.as_u16()), clip(&message, 200))
}

fn net_error(e: &reqwest::Error) -> AiError {
    let mut message = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        message = format!("{message}: {s}");
        src = s.source();
    }
    AiError::new(if e.is_timeout() { "TIMEOUT" } else { "NETWORK" }, clip(&message, 200))
}

/// Request + routing info gathered from the active session under the lock.
struct Job {
    req: Req,
    provider: Provider,
    kind: Kind,
    lens: String,
    proxy: Option<String>,
}

fn job(app: &AppHandle, id: &str, idx: usize) -> Option<Job> {
    let s = state::settings(app);
    let g = lock(&ai(app).active);
    let a = g.as_ref().filter(|a| a.view.id == id && !a.cancel.is_cancelled())?;
    let p = &a.provider;
    let mut turns = vec![Turn { role: Role::User, text: a.first_user.clone(), raw: None }];
    for (i, m) in a.view.messages.iter().enumerate().take(idx).skip(1) {
        let raw = a.raw_assistant.get(i).cloned().flatten();
        // A failed turn with no text would be an empty message (400 on Anthropic): skip it.
        if m.role == Role::Assistant && m.text.is_empty() && raw.is_none() {
            continue;
        }
        turns.push(Turn { role: m.role, text: m.text.clone(), raw });
    }
    let model = match a.view.kind {
        Kind::Text => p.model.clone(),
        _ => p.vision_model.clone().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| p.model.clone()),
    };
    Some(Job {
        req: Req {
            base_url: p.base_url.clone(),
            model,
            key: None,
            system: a.system.clone(),
            turns,
            image: a.image.clone(),
            max_tokens: s.max_tokens,
            extra: p.extra_body.clone(),
        },
        provider: p.clone(),
        kind: a.view.kind,
        lens: a.view.lens.clone(),
        proxy: s.proxy,
    })
}

/// Streams into one assistant message; batches deltas to ≤ 1 emit per 40 ms.
struct Sink<'a> {
    app: &'a AppHandle,
    id: &'a str,
    idx: usize,
    /// Scan's first answer: buffer only, no deltas.
    silent: bool,
    text: String,
    pending: String,
    last_emit: Option<Instant>,
    /// Request sent (TTFT origin).
    sent: Instant,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Sink<'_> {
    fn push(&mut self, t: &str) {
        let now = Instant::now();
        if self.first.is_none() {
            self.first = Some(now);
            if !self.silent {
                telemetry::set_mode(self.app, HudMode::Stream);
            }
        }
        self.last = Some(now);
        self.text.push_str(t);
        if !self.silent {
            self.pending.push_str(t);
        }
    }

    fn due(&self) -> bool {
        self.last_emit.is_none_or(|t| t.elapsed() >= BATCH)
    }

    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.pending);
        let (idx, t) = (self.idx, text.clone());
        let hit = with_active(self.app, self.id, |a| {
            if let Some(m) = a.view.messages.get_mut(idx) {
                m.text.push_str(&t);
            }
        });
        if let Some((lens, ())) = hit {
            emit(self.app, &lens, SessionEvent::Delta { id: self.id.into(), msg_index: idx, text });
        }
        self.last_emit = Some(Instant::now());
    }
}

/// One request task: stream the answer into `messages[idx]`. Silent if cancelled.
async fn run(app: AppHandle, id: String, idx: usize, cancel: CancellationToken) {
    let Some(mut job) = job(&app, &id, idx) else { return };
    let scan_first = job.kind == Kind::Scan && idx == 1;
    telemetry::set_mode(&app, if scan_first { HudMode::Scan } else { HudMode::Thinking });
    let mut sink = Sink {
        app: &app,
        id: &id,
        idx,
        silent: scan_first,
        text: String::new(),
        pending: String::new(),
        last_emit: None,
        sent: Instant::now(),
        first: None,
        last: None,
    };
    let mut parsed = Parsed::default();
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return,
        r = stream(&mut job, &mut sink, &mut parsed) => r,
    };
    if cancel.is_cancelled() {
        return;
    }
    let result = result.and_then(|()| match parsed.stop {
        Stop::Refused => Err(AiError::new(
            "DECLINED",
            match &parsed.detail {
                Some(c) => format!("The model declined to answer ({c})."),
                None => "The model declined to answer.".into(),
            },
        )),
        _ => Ok(()),
    });
    if let Err(e) = result {
        sink.flush();
        if !sink.silent && !sink.text.is_empty() {
            sink.push(" ⚠ interrupted");
            sink.flush();
        }
        let hit = with_active(&app, &id, |a| {
            a.view.streaming = false;
            a.view.error = Some(e.code.clone());
        });
        if let Some((lens, ())) = hit {
            emit(&app, &lens, SessionEvent::Error { id: id.clone(), msg_index: idx, code: e.code, message: e.message });
            telemetry::set_mode(&app, HudMode::Standby);
        }
        return;
    }

    let output = parsed.output_tokens.unwrap_or_else(|| (sink.text.chars().count() as u64).div_ceil(4));
    let ttft = sink.first.map(|f| f.duration_since(sink.sent).as_millis() as u64);
    let tps = match (sink.first, sink.last) {
        (Some(f), Some(l)) if l > f => Some(output as f64 / (l - f).as_secs_f64()),
        _ => None,
    };
    telemetry::record_usage(&app, output, ttft, tps);
    if parsed.stop == Stop::Length && !sink.silent {
        sink.push("…");
    }
    sink.flush();
    let raw = match job.provider.kind {
        ProviderKind::Anthropic => anthropic::replay_blocks(std::mem::take(&mut parsed.blocks)),
        ProviderKind::Openai => None,
    };
    if scan_first {
        finish_scan(&app, &id, &sink.text, raw, job);
        return;
    }
    let hit = with_active(&app, &id, |a| {
        a.view.streaming = false;
        if let Some(r) = a.raw_assistant.get_mut(idx) {
            *r = raw;
        }
    });
    if let Some((lens, ())) = hit {
        emit(&app, &lens, SessionEvent::Done { id: id.clone(), msg_index: idx });
        telemetry::set_mode(&app, HudMode::Standby);
    }
}

/// Send the request and pump SSE events into `sink`/`parsed` until the stream ends.
async fn stream(job: &mut Job, sink: &mut Sink<'_>, parsed: &mut Parsed) -> Result<(), AiError> {
    job.req.key = api_key(&job.provider)?;
    let client = client(&job.req.base_url, job.proxy.as_deref())?;
    let builder = match job.provider.kind {
        ProviderKind::Openai => openai::request(&client, &job.req, true),
        ProviderKind::Anthropic => anthropic::request(&client, &job.req, true),
    };
    sink.sent = Instant::now();
    let resp = tokio::time::timeout(first_byte_timeout(&job.req.base_url), builder.send())
        .await
        .map_err(|_| AiError::new("TIMEOUT", "no response from the provider"))?
        .map_err(|e| net_error(&e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = tokio::time::timeout(IDLE, resp.text()).await.ok().and_then(Result::ok).unwrap_or_default();
        return Err(http_error(status, &body));
    }
    let mut events = std::pin::pin!(resp.bytes_stream().eventsource());
    loop {
        let flush_at = sink.last_emit.unwrap_or_else(Instant::now) + BATCH;
        tokio::select! {
            _ = tokio::time::sleep_until(flush_at.into()), if !sink.pending.is_empty() => sink.flush(),
            ev = tokio::time::timeout(IDLE, events.next()) => {
                let ev = match ev {
                    Err(_) => return Err(AiError::new("TIMEOUT", "the stream stalled for 30 s")),
                    Ok(None) if parsed.done || parsed.finished => return Ok(()),
                    Ok(None) => return Err(AiError::new("NETWORK", "connection closed mid-answer")),
                    Ok(Some(Err(eventsource_stream::EventStreamError::Transport(e)))) => return Err(net_error(&e)),
                    Ok(Some(Err(e))) => return Err(AiError::new("NETWORK", clip(&e.to_string(), 200))),
                    Ok(Some(Ok(ev))) => ev,
                };
                let text = match job.provider.kind {
                    ProviderKind::Openai => openai::feed(&ev.data, parsed)?,
                    ProviderKind::Anthropic => anthropic::feed(&ev.data, parsed)?,
                };
                if let Some(t) = text {
                    sink.push(&t);
                    if sink.due() {
                        sink.flush();
                    }
                }
                if parsed.done {
                    return Ok(());
                }
            }
        }
    }
}

/// Map the model's scan reply to items (stage A list + stage B anchors) or toast + dismiss.
fn finish_scan(app: &AppHandle, id: &str, text: &str, raw: Option<Value>, job: Job) {
    let items = match prompts::parse_scan(text) {
        Some(items) if !items.is_empty() => items,
        other => {
            if dismiss(app, id) {
                actions::toast(app, if other.is_some() { "NO POINTS FOUND" } else { "SCAN FAILED" });
            }
            return;
        }
    };
    let lens = windows::lens_by_label(app, &job.lens);
    let items: Vec<ScanItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, it)| ScanItem {
            key: i.to_string(),
            rect: match (it.bbox, &job.req.image, &lens) {
                (Some(b), Some(img), Some(lens)) => map_box(b, job.provider.bbox_format, img, lens),
                _ => None,
            },
            label: it.label,
            brief: it.brief,
            detail: it.detail,
        })
        .collect();
    let hit = with_active(app, id, |a| {
        a.view.streaming = false;
        a.view.items = Some(items.clone());
        // The raw JSON stays as the answer so follow-ups have the context.
        if let Some(m) = a.view.messages.get_mut(1) {
            m.text = text.to_string();
        }
        if let Some(r) = a.raw_assistant.get_mut(1) {
            *r = raw;
        }
    });
    if let Some((lens, ())) = hit {
        emit(app, &lens, SessionEvent::Scan { id: id.into(), items });
        telemetry::set_mode(app, HudMode::Standby);
    }
}

/// Model box (image pixels or 0–1000) -> lens CSS px. None if the box is unusable.
fn map_box(b: [f64; 4], fmt: BboxFormat, img: &ImagePayload, lens: &LensInfo) -> Option<Rect> {
    let (lim_x, lim_y) = match fmt {
        BboxFormat::Pixel => (img.w as f64, img.h as f64),
        BboxFormat::Norm1000 => (1000.0, 1000.0),
    };
    let [x1, y1, x2, y2] = b;
    if b.iter().any(|v| !v.is_finite()) || x2 <= x1 || y2 <= y1 || x1 >= lim_x || y1 >= lim_y || x2 <= 0.0 || y2 <= 0.0 {
        return None;
    }
    let (sx, sy) = (img.screen.w as f64 / lim_x, img.screen.h as f64 / lim_y);
    let (x1, y1, x2, y2) = (x1.max(0.0) * sx, y1.max(0.0) * sy, x2.min(lim_x) * sx, y2.min(lim_y) * sy);
    let phys = PhysRect {
        x: img.screen.x + x1.round() as i32,
        y: img.screen.y + y1.round() as i32,
        w: ((x2 - x1).round() as i32).max(1),
        h: ((y2 - y1).round() as i32).max(1),
    };
    Some(lens.to_css(phys))
}
