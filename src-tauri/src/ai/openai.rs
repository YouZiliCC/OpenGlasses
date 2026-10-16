//! OpenAI-compatible Chat Completions with SSE (`POST {baseUrl}/chat/completions`).
//! Covers OpenAI, DeepSeek, DashScope, OpenRouter, Ollama, vLLM, LM Studio…

use super::{merge_patch, stream_error, AiError, Parsed, Req, Stop};
use crate::types::Role;
use serde_json::{json, Value};

pub(super) fn request(c: &reqwest::Client, r: &Req) -> reqwest::RequestBuilder {
    let url = format!("{}/chat/completions", r.base_url.trim_end_matches('/'));
    let b = c.post(url).json(&body(r));
    match &r.key {
        Some(k) => b.bearer_auth(k),
        None => b, // keyless local servers (Ollama…)
    }
}

fn body(r: &Req) -> Value {
    let mut messages = Vec::with_capacity(r.turns.len() + 1);
    if !r.system.is_empty() {
        messages.push(json!({"role": "system", "content": r.system}));
    }
    for (i, t) in r.turns.iter().enumerate() {
        messages.push(match (t.role, &r.image) {
            (Role::User, Some(img)) if i == 0 => json!({"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": format!("data:{};base64,{}", img.mime, img.base64)}},
                {"type": "text", "text": t.text},
            ]}),
            (Role::User, _) => json!({"role": "user", "content": t.text}),
            (Role::Assistant, _) => json!({"role": "assistant", "content": t.text}),
        });
    }
    let mut b = json!({"model": r.model, "messages": messages, "max_tokens": r.max_tokens, "stream": true,
        "stream_options": {"include_usage": true}});
    if let Some(extra) = &r.extra {
        merge_patch(&mut b, extra);
    }
    b
}

/// One SSE `data:` payload. Returns visible text, if any.
pub(super) fn feed(data: &str, p: &mut Parsed) -> Result<Option<String>, AiError> {
    if data.trim() == "[DONE]" {
        p.done = true;
        return Ok(None);
    }
    let Ok(v) = serde_json::from_str::<Value>(data) else { return Ok(None) };
    if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
        return Err(stream_error(e));
    }
    // include_usage: the final chunk has empty `choices` and the totals.
    if let Some(n) = v["usage"]["completion_tokens"].as_u64() {
        p.output_tokens = Some(n);
    }
    let c = &v["choices"][0];
    if let Some(reason) = c["finish_reason"].as_str() {
        p.finished = true;
        p.stop = match reason {
            "length" => Stop::Length,
            "content_filter" => Stop::Refused,
            _ => Stop::Normal,
        };
    }
    Ok(c["delta"]["content"].as_str().filter(|s| !s.is_empty()).map(str::to_owned))
}
