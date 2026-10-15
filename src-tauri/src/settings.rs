//! settings.json (serde_json, temp file + rename), usage.json, presets, and API keys in the
//! Windows Credential Manager via `keyring` (service "openglasses", user = slot id).
//! DESIGN §3.5 (presets/extraBody) and §3.7.

use crate::types::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const KEYRING_SERVICE: &str = "openglasses";

const SETTINGS_FILE: &str = "settings.json";
const USAGE_FILE: &str = "usage.json";

/// Serializes read-modify-write of usage.json.
static USAGE_LOCK: Mutex<()> = Mutex::new(());

#[allow(clippy::too_many_arguments)]
fn slot(
    id: &str,
    name: &str,
    kind: ProviderKind,
    base_url: &str,
    model: &str,
    vision_model: Option<&str>,
    extra_body: Option<Value>,
    bbox_format: BboxFormat,
) -> Provider {
    Provider {
        id: id.into(),
        name: name.into(),
        kind,
        base_url: base_url.into(),
        model: model.into(),
        vision_model: vision_model.map(Into::into),
        extra_body: extra_body.and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        }),
        bbox_format,
    }
}

/// The 8 fixed provider slots + all other defaults.
pub fn defaults() -> Settings {
    use BboxFormat::*;
    use ProviderKind::*;
    let effort = || Some(json!({ "output_config": { "effort": "low" } }));
    let empty = || Some(json!({}));
    let openai_extra =
        json!({ "max_tokens": null, "max_completion_tokens": 2000, "reasoning_effort": "none" });
    #[rustfmt::skip]
    let providers = vec![
        slot("openai", "OpenAI", Openai, "https://api.openai.com/v1", "gpt-6-luna", None, Some(openai_extra), Pixel),
        slot("anthropic", "Anthropic", Anthropic, "https://api.anthropic.com", "claude-opus-5-5", None, effort(), Pixel),
        slot("deepseek", "DeepSeek", Openai, "https://api.deepseek.com/v1", "deepseek-chat", None, empty(), Pixel),
        slot("qwen", "Qwen (DashScope)", Openai, "https://dashscope.aliyuncs.com/compatible-mode/v1", "qwen-plus",
            Some("qwen3-vl-plus"), Some(json!({ "enable_thinking": false })), Norm1000),
        slot("openrouter", "OpenRouter", Openai, "https://openrouter.ai/api/v1", "anthropic/claude-haiku-5-5", None, empty(), Pixel),
        slot("ollama", "Ollama (local)", Openai, "http://localhost:11434/v1", "qwen3-vl:8b", None, empty(), Norm1000),
        slot("custom-openai", "Custom (OpenAI)", Openai, "", "", None, None, Pixel),
        slot("custom-anthropic", "Custom (Anthropic)", Anthropic, "https://api.anthropic.com", "claude-opus-5-5", None, effort(), Pixel),
    ];
    Settings {
        providers,
        active_provider_id: "anthropic".into(),
        hotkeys: [
            (HotkeyAction::Explain, "Alt+Q"),
            (HotkeyAction::Capture, "Alt+W"),
            (HotkeyAction::Scan, "Alt+E"),
            (HotkeyAction::Toggle, "Ctrl+Alt+G"),
        ]
        .into_iter()
        .map(|(a, k)| (a, k.to_string()))
        .collect(),
        style: Style::Lockon,
        hud_corners: HudCorners {
            tl: true,
            tr: true,
            bl: true,
            br: true,
        },
        lens_on: true,
        glass: GlassMode::Snapshot,
        answer_language: "auto".into(),
        include_window_title: true,
        proxy: None,
        max_tokens: 2000,
        exclude_from_capture: true,
        theme: "terminal".into(),
        anim_speed: AnimSpeed::Normal,
    }
}

/// Load from the app config dir; missing/corrupt file -> defaults. Always returns exactly the
/// 8 slots (fill missing slots from defaults, drop unknown ids).
pub fn load(app: &AppHandle) -> Settings {
    let user = config_path(app, SETTINGS_FILE)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    match user {
        Some(Value::Object(user)) => merge(&user),
        _ => defaults(),
    }
}

/// Overlay a user's (possibly partial / outdated) settings object onto the defaults.
/// A field that would not deserialize keeps its default.
fn merge(user: &Map<String, Value>) -> Settings {
    let d = defaults();
    let mut base = serde_json::to_value(&d).expect("defaults serialize");
    overlay::<Settings>(&mut base, user, &["providers"]);

    let user_slots = user.get("providers").and_then(Value::as_array);
    let providers = d
        .providers
        .iter()
        .map(|p| {
            let found = user_slots.and_then(|a| {
                a.iter()
                    .filter_map(Value::as_object)
                    .find(|u| u.get("id").and_then(Value::as_str) == Some(p.id.as_str()))
            });
            let Some(u) = found else { return p.clone() };
            // id/name/kind are fixed per slot; only the editable fields come from the file.
            let mut v = serde_json::to_value(p).expect("provider serializes");
            overlay::<Provider>(&mut v, u, &["id", "name", "kind"]);
            serde_json::from_value(v).unwrap_or_else(|_| p.clone())
        })
        .collect();

    let mut s: Settings = serde_json::from_value(base).unwrap_or(d);
    s.providers = providers;
    if !s.providers.iter().any(|p| p.id == s.active_provider_id) {
        s.active_provider_id = defaults().active_provider_id;
    }
    s
}

/// Copy `user`'s values for the keys `base` already has (except `skip`), one key at a time,
/// keeping a key only if `base` still deserializes as `T`. Nested objects (hotkeys, hudCorners)
/// merge one level; `extraBody` is replaced as a whole.
fn overlay<T: DeserializeOwned>(base: &mut Value, user: &Map<String, Value>, skip: &[&str]) {
    let keys: Vec<String> = base
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    for k in keys {
        let Some(v) = user.get(&k).filter(|_| !skip.contains(&k.as_str())) else {
            continue;
        };
        let mut trial = base.clone();
        match (&mut trial[&k], v) {
            (Value::Object(dst), Value::Object(src)) if k != "extraBody" => {
                for (sk, sv) in src {
                    if dst.contains_key(sk) {
                        dst.insert(sk.clone(), sv.clone());
                    }
                }
            }
            (dst, v) => *dst = v.clone(),
        }
        if serde_json::from_value::<T>(trial.clone()).is_ok() {
            *base = trial;
        }
    }
}

/// Persist atomically and update `AppState.settings`. Does not emit or re-register anything.
pub fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    write_atomic(&config_path(app, SETTINGS_FILE)?, &bytes)?;
    *crate::state::st(app).settings.write().unwrap() = s.clone();
    Ok(())
}

fn config_path(app: &AppHandle, file: &str) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|d| d.join(file))
        .map_err(|e| e.to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    // std::fs::rename replaces an existing target on Windows (MOVEFILE_REPLACE_EXISTING).
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn active_provider(s: &Settings) -> Provider {
    s.providers
        .iter()
        .find(|p| p.id == s.active_provider_id)
        .or(s.providers.first())
        .cloned()
        .unwrap_or_else(|| defaults().providers.swap_remove(1))
}

/// True if a request can be attempted: key present, or baseUrl is localhost/127.0.0.1.
pub fn provider_usable(p: &Provider) -> bool {
    is_local_url(&p.base_url) || has_api_key(&p.id)
}

/// baseUrl points at this machine (localhost, 127.0.0.1, [::1]).
pub(crate) fn is_local_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .is_some_and(|u| matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
}

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, id).map_err(|e| e.to_string())
}

pub fn get_api_key(id: &str) -> Option<String> {
    entry(id)
        .ok()?
        .get_password()
        .ok()
        .filter(|k| !k.trim().is_empty())
}

pub fn set_api_key(id: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return delete_api_key(id);
    }
    entry(id)?.set_password(key).map_err(|e| e.to_string())
}

pub fn has_api_key(id: &str) -> bool {
    get_api_key(id).is_some()
}

pub fn delete_api_key(id: &str) -> Result<(), String> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Serialize, Deserialize)]
struct Usage {
    date: String,
    tokens: u64,
}

fn today() -> String {
    // NaiveDate's Display is "YYYY-MM-DD".
    chrono::Local::now().date_naive().to_string()
}

/// Today's usage record (a fresh one if the file is missing, corrupt or from another day).
fn read_usage(app: &AppHandle) -> Usage {
    let date = today();
    config_path(app, USAGE_FILE)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Usage>(&b).ok())
        .filter(|u| u.date == date)
        .unwrap_or(Usage { date, tokens: 0 })
}

/// Add tokens to today's counter in usage.json; returns today's total.
pub fn add_usage(app: &AppHandle, tokens: u64) -> u64 {
    let _g = USAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut u = read_usage(app);
    u.tokens = u.tokens.saturating_add(tokens);
    let written = serde_json::to_vec(&u)
        .map_err(|e| e.to_string())
        .and_then(|b| write_atomic(&config_path(app, USAGE_FILE)?, &b));
    if let Err(e) = written {
        log::warn!("usage.json: {e}");
    }
    u.tokens
}

/// Today's total from usage.json (0 if another day).
pub fn usage_today(app: &AppHandle) -> u64 {
    let _g = USAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    read_usage(app).tokens
}
