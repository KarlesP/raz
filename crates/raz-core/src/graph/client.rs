//! Thin Microsoft Graph v1.0 REST client over reqwest. Mirrors the status-to-[`RazError`]
//! mapping of the ARM client so commands inherit az-compatible exit codes.

use serde_json::Value;

use crate::error::{RazError, Result};

pub struct GraphClient {
    http: reqwest::Client,
    token: String,
    /// Microsoft Graph v1.0 base URL for the active cloud.
    base: String,
    trace: bool,
}

impl GraphClient {
    pub fn new(http: reqwest::Client, token: String, base: String) -> Self {
        Self {
            http,
            token,
            base,
            trace: false,
        }
    }

    /// Enable request tracing to stderr (az `--debug`).
    pub fn trace(mut self, trace: bool) -> Self {
        self.trace = trace;
        self
    }

    fn log(&self, method: &str, url: &str) {
        if self.trace {
            eprintln!("raz: → {method} {url}");
        }
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        self.log("GET", &url);
        let resp = self.http.get(url).bearer_auth(&self.token).send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp.json::<Value>().await?);
        }
        Err(map_status(
            status.as_u16(),
            path,
            resp.text().await.unwrap_or_default(),
        ))
    }

    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        self.log("POST", &url);
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Ok(serde_json::from_str(&text).unwrap_or(Value::Null));
        }
        Err(map_status(
            status.as_u16(),
            path,
            resp.text().await.unwrap_or_default(),
        ))
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{path}", self.base);
        self.log("DELETE", &url);
        let resp = self
            .http
            .delete(url)
            .bearer_auth(&self.token)
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        Err(map_status(
            status.as_u16(),
            path,
            resp.text().await.unwrap_or_default(),
        ))
    }
}

fn map_status(status: u16, path: &str, body: String) -> RazError {
    crate::error::map_http_status("Graph", status, path, body)
}
