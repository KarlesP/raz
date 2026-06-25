//! Minimal OpenAI-compatible chat client for the advisor's `--summary`. Works against any server
//! that speaks `/chat/completions` — llama.cpp `llama-server`, mistral.rs, vLLM, Ollama, etc. The
//! embedded candle GGUF backend (Phase 2b) will sit behind the same `complete` facade.

use serde_json::{json, Value};

use crate::error::{RazError, Result};

/// Call `{base}/chat/completions` and return the assistant message. `base` is the OpenAI-style
/// root (e.g. `http://localhost:8080/v1`); `api_key` is optional (local servers ignore it).
pub async fn complete(
    http: &reqwest::Client,
    base: &str,
    model: &str,
    api_key: Option<&str>,
    system: &str,
    user: &str,
) -> Result<String> {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
        "temperature": 0.2,
        "max_tokens": 800,
        "stream": false,
    });
    let mut req = http.post(&url).json(&body);
    if let Some(key) = api_key {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| {
        RazError::Http(format!(
            "LLM endpoint unreachable ({url}) — is the server running? {e}"
        ))
    })?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(RazError::Http(format!(
            "LLM endpoint {}: {text}",
            status.as_u16()
        )));
    }
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v.pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .ok_or_else(|| RazError::Other("LLM response had no message content".into()))
}
