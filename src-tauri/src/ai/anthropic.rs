//! Anthropic Messages API with SSE (`POST {baseUrl}/v1/messages`).
//! Headers: x-api-key, anthropic-version: 2023-06-01. Never send thinking/temperature/top_p.
//! Replay assistant content blocks verbatim (incl. thinking + signature) on follow-ups.

use super::{host, merge_patch, stream_error, AiError, Parsed, Req, Stop};
use crate::types::Role;
use serde_json::{json, Value};

/// Models that accept server-side fallbacks (`fallbacks: "default"`), official API only.
const FALLBACK_MODELS: [&str; 4] = ["claude-fable-5-1", "claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5"];
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

pub(super) fn request(c: &reqwest::Client, r: &Req, stream: bool) -> reqwest::RequestBuilder {
    let base = r.base_url.trim_end_matches('/');
    let base = base.strip_suffix("/v1").unwrap_or(base);
    let fallbacks = FALLBACK_MODELS.contains(&r.model.as_str()) && host(&r.base_url) == "api.anthropic.com";
    let mut b = c
        .post(format!("{base}/v1/messages"))
        .header("anthropic-version", "2023-06-01")
        .json(&body(r, stream, fallbacks));
    if let Some(k) = &r.key {
        b = b.header("x-api-key", k);
    }
    if fallbacks {
        b = b.header("anthropic-beta", FALLBACK_BETA);
    }
    b
}

fn body(r: &Req, stream: bool, fallbacks: bool) -> Value {
    let messages: Vec<Value> = r
        .turns
        .iter()
        .enumerate()
        .map(|(i, t)| match t.role {
            Role::User if i == 0 => {
                let mut content = Vec::with_capacity(2);
                if let Some(img) = &r.image {
                    content.push(json!({"type": "image", "source":
                        {"type": "base64", "media_type": img.mime, "data": img.base64}}));
                }
                content.push(json!({"type": "text", "text": t.text}));
                json!({"role": "user", "content": content})
            }
            Role::User => json!({"role": "user", "content": t.text}),
            Role::Assistant => json!({"role": "assistant", "content": t.raw.clone().unwrap_or_else(|| json!(t.text))}),
        })
        .collect();
    let mut b = json!({"model": r.model, "max_tokens": r.max_tokens, "messages": messages, "stream": stream});
    if !r.system.is_empty() {
        b["system"] = json!(r.system);
    }
    if fallbacks {
        b["fallbacks"] = json!("default");
    }
    if let Some(extra) = &r.extra {
        merge_patch(&mut b, extra);
    }
    b
}

/// One SSE event payload (its `type` field mirrors the SSE event name). Rebuilds the content
/// blocks in `p.blocks`; returns visible text (text_delta only).
pub(super) fn feed(data: &str, p: &mut Parsed) -> Result<Option<String>, AiError> {
    let Ok(v) = serde_json::from_str::<Value>(data) else { return Ok(None) };
    let index = v["index"].as_u64().unwrap_or(0) as usize;
    match v["type"].as_str().unwrap_or("") {
        "content_block_start" => {
            let block = v["content_block"].clone();
            match index.cmp(&p.blocks.len()) {
                std::cmp::Ordering::Less => p.blocks[index] = block,
                _ => p.blocks.push(block),
            }
        }
        "content_block_delta" => {
            let (d, Some(b)) = (&v["delta"], p.blocks.get_mut(index)) else { return Ok(None) };
            let (key, text) = match d["type"].as_str().unwrap_or("") {
                "text_delta" => ("text", &d["text"]),
                "thinking_delta" => ("thinking", &d["thinking"]),
                "signature_delta" => ("signature", &d["signature"]),
                _ => return Ok(None),
            };
            let text = text.as_str().unwrap_or("");
            if let Some(o) = b.as_object_mut() {
                match o.get_mut(key) {
                    Some(Value::String(s)) => s.push_str(text),
                    _ => {
                        o.insert(key.into(), json!(text));
                    }
                }
            }
            if key == "text" && !text.is_empty() {
                return Ok(Some(text.to_string()));
            }
        }
        "message_delta" => {
            if let Some(reason) = v["delta"]["stop_reason"].as_str() {
                p.finished = true;
                p.stop = match reason {
                    "max_tokens" => Stop::Length,
                    "refusal" => Stop::Refused,
                    _ => Stop::Normal,
                };
                p.detail = v["delta"]["stop_details"]["category"].as_str().map(str::to_owned);
            }
            if let Some(n) = v["usage"]["output_tokens"].as_u64() {
                p.output_tokens = Some(n);
            }
        }
        "message_stop" => p.done = true,
        "error" => return Err(stream_error(&v["error"])),
        _ => {} // message_start, content_block_stop, ping
    }
    Ok(None)
}

/// Blocks to store for replay, or None if there is no visible text to anchor them.
/// Before the last `fallback` marker only text survives (the declined model's thinking must
/// not be echoed back); empty text blocks are dropped (400 on replay); markers are dropped.
pub(super) fn replay_blocks(blocks: Vec<Value>) -> Option<Value> {
    let last_fallback = blocks.iter().rposition(|b| b["type"] == "fallback");
    let kept: Vec<Value> = blocks
        .into_iter()
        .enumerate()
        .filter(|(i, b)| match b["type"].as_str().unwrap_or("") {
            "text" => b["text"].as_str().is_some_and(|s| !s.is_empty()),
            "fallback" => false,
            _ => last_fallback.is_none_or(|f| *i > f),
        })
        .map(|(_, b)| b)
        .collect();
    kept.iter().any(|b| b["type"] == "text").then_some(Value::Array(kept))
}

/// Text of a non-streaming reply (`test_provider`).
pub(super) fn reply_text(v: &Value) -> Result<String, AiError> {
    if v["stop_reason"] == "refusal" {
        return Err(AiError::new("DECLINED", "the model declined the request"));
    }
    let blocks = v["content"].as_array().map(Vec::as_slice).unwrap_or_default();
    Ok(blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect())
}
