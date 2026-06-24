//! Managed-identity sign-in for `raz login --identity`, matching how the Azure CLI (via MSAL's
//! `ManagedIdentityClient`) detects the token source, in precedence order:
//!
//! - App Service / Functions — `IDENTITY_ENDPOINT` + `IDENTITY_HEADER` (api 2019-08-01)
//! - Cloud Shell / legacy App Service — `MSI_ENDPOINT` (+ `MSI_SECRET`) (api 2017-09-01)
//! - IMDS (VM/VMSS) — `http://169.254.169.254/...` (api 2018-02-01)
//!
//! User-assigned identity is selected by client_id, object_id, or resource_id. No refresh token —
//! the cached ARM access token is used directly. (Azure Arc's challenge flow is not supported.)

use std::env;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use serde_json::Value;

use super::device_code::TokenResponse;
use super::now_unix;
use crate::error::{RazError, Result};

const IMDS_TOKEN_URL: &str = "http://169.254.169.254/metadata/identity/oauth2/token";
const ARM_RESOURCE: &str = "https://management.azure.com/";

#[derive(Deserialize)]
struct ImdsToken {
    access_token: String,
    // Endpoints return these as strings.
    expires_in: Option<String>,
    expires_on: Option<String>,
}

/// Resolve the token endpoint, api-version, and required headers from the environment, mirroring
/// the Azure CLI's source detection.
fn endpoint() -> (String, &'static str, Vec<(&'static str, String)>) {
    if let (Ok(ep), Ok(header)) = (env::var("IDENTITY_ENDPOINT"), env::var("IDENTITY_HEADER")) {
        return (ep, "2019-08-01", vec![("X-IDENTITY-HEADER", header)]);
    }
    if let Ok(ep) = env::var("MSI_ENDPOINT") {
        let mut headers = vec![("Metadata", "true".to_string())];
        if let Ok(secret) = env::var("MSI_SECRET") {
            headers.push(("Secret", secret));
        }
        return (ep, "2017-09-01", headers);
    }
    (
        IMDS_TOKEN_URL.to_string(),
        "2018-02-01",
        vec![("Metadata", "true".to_string())],
    )
}

/// The user-assigned identity selector, if any (client_id wins, then object_id, then resource_id).
fn id_query(
    client_id: Option<&str>,
    object_id: Option<&str>,
    resource_id: Option<&str>,
) -> Vec<(&'static str, String)> {
    if let Some(c) = client_id {
        vec![("client_id", c.to_string())]
    } else if let Some(o) = object_id {
        vec![("object_id", o.to_string())]
    } else if let Some(r) = resource_id {
        vec![("mi_res_id", r.to_string())]
    } else {
        vec![]
    }
}

/// Acquire an ARM token from the detected managed-identity endpoint.
pub async fn acquire(
    http: &reqwest::Client,
    client_id: Option<&str>,
    object_id: Option<&str>,
    resource_id: Option<&str>,
) -> Result<TokenResponse> {
    let (url, api_version, headers) = endpoint();
    let mut query = vec![
        ("api-version", api_version.to_string()),
        ("resource", ARM_RESOURCE.to_string()),
    ];
    query.extend(id_query(client_id, object_id, resource_id));

    let mut req = http.get(&url);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req.query(&query).send().await.map_err(|e| {
        RazError::Auth(format!(
            "managed-identity endpoint unreachable — `--identity` only works on an Azure-hosted resource: {e}"
        ))
    })?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        return Err(RazError::Auth(format!(
            "managed-identity token request failed ({status}): {}",
            resp.text().await.unwrap_or_default()
        )));
    }
    let raw: ImdsToken = resp.json().await?;
    let expires_in = raw
        .expires_in
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            raw.expires_on
                .and_then(|s| s.parse::<i64>().ok())
                .map(|on| on - now_unix())
        })
        .unwrap_or(3600);
    Ok(TokenResponse {
        access_token: raw.access_token,
        refresh_token: None,
        expires_in,
    })
}

/// Extract the tenant id (`tid`) from a JWT access token's payload, like az does to set the
/// active tenant. Decodes only — no signature verification.
pub fn tenant_from_token(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims
        .get("tid")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tid_from_jwt() {
        // header.payload.signature; payload = {"tid":"my-tenant"} base64url-no-pad.
        let payload = URL_SAFE_NO_PAD.encode(br#"{"tid":"my-tenant"}"#);
        let jwt = format!("h.{payload}.s");
        assert_eq!(tenant_from_token(&jwt).as_deref(), Some("my-tenant"));
        assert_eq!(tenant_from_token("not-a-jwt"), None);
    }
}
