//! Thin Microsoft Graph v1.0 REST client over reqwest. Mirrors the status-to-[`RazError`]
//! mapping of the ARM client so commands inherit az-compatible exit codes.

use serde_json::Value;

use crate::error::{RazError, Result};

const GRAPH_ENDPOINT: &str = "https://graph.microsoft.com/v1.0";

pub struct GraphClient {
    http: reqwest::Client,
    token: String,
}

impl GraphClient {
    pub fn new(http: reqwest::Client, token: String) -> Self {
        Self { http, token }
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        let resp = self
            .http
            .get(format!("{GRAPH_ENDPOINT}{path}"))
            .bearer_auth(&self.token)
            .send()
            .await?;
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
        let resp = self
            .http
            .post(format!("{GRAPH_ENDPOINT}{path}"))
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
        let resp = self
            .http
            .delete(format!("{GRAPH_ENDPOINT}{path}"))
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
