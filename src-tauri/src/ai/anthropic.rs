//! Anthropic Messages API with SSE (`POST {baseUrl}/v1/messages`).
//! Headers: x-api-key, anthropic-version: 2023-06-01. Never send thinking/temperature/top_p.
//! Replay assistant content blocks verbatim (incl. thinking + signature) on follow-ups.
