//! settings.json (serde_json, temp file + rename), usage.json, presets, and API keys in the
//! Windows Credential Manager via `keyring` (service "openglasses", user = slot id).
//! DESIGN §3.5 (presets/extraBody) and §3.7.

use crate::types::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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

/// Load `%APPDATA%\OpenGlasses\settings.json`; a missing or invalid file -> defaults.
pub fn load(app: &AppHandle) -> Settings {
    let Some(b) = config_path(app, SETTINGS_FILE).ok().and_then(|p| std::fs::read(p).ok()) else {
        return defaults();
    };
    match serde_json::from_slice::<Settings>(&b) {
        Ok(s) if valid(&s) => s,
        Ok(_) => {
            log::warn!("settings.json: unknown activeProviderId, using defaults");
            defaults()
        }
        Err(e) => {
            log::warn!("settings.json ignored: {e}");
            defaults()
        }
    }
}

/// The active provider id names an existing slot.
pub fn valid(s: &Settings) -> bool {
    s.providers.iter().any(|p| p.id == s.active_provider_id)
}

/// Persist atomically and update `AppState.settings`. Does not emit or re-register anything.
pub fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    write_atomic(&config_path(app, SETTINGS_FILE)?, &bytes)?;
    *crate::state::st(app).settings.write().unwrap() = s.clone();
    Ok(())
}

fn config_path(app: &AppHandle, file: &str) -> Result<PathBuf, String> {
    user_dir(app).map(|d| d.join(file))
}

/// `%APPDATA%\OpenGlasses`: settings.json, usage.json and themes/.
pub fn user_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().config_dir().map(|d| d.join("OpenGlasses")).map_err(|e| e.to_string())
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

/// The active slot; `load` and `apply_settings` only accept settings where it exists (`valid`).
pub fn active_provider(s: &Settings) -> Provider {
    s.providers
        .iter()
        .find(|p| p.id == s.active_provider_id)
        .cloned()
        .expect("validated by load/apply_settings")
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
    entry(id).ok()?.get_password().ok()
}

pub fn set_api_key(id: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("empty API key".into());
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
    let _g = USAGE_LOCK.lock().unwrap();
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
    let _g = USAGE_LOCK.lock().unwrap();
    read_usage(app).tokens
}
