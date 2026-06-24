//! Managed-identity sign-in via the Azure Instance Metadata Service (IMDS), for `raz login
//! --identity` on Azure-hosted resources (VMs, runners, App Service). No refresh token — the
//! cached ARM access token is used directly until it expires.

use serde::Deserialize;

use super::device_code::TokenResponse;
use super::now_unix;
use crate::error::{RazError, Result};

const IMDS_TOKEN_URL: &str = "http://169.254.169.254/metadata/identity/oauth2/token";
const ARM_RESOURCE: &str = "https://management.azure.com/";

#[derive(Deserialize)]
struct ImdsToken {
    access_token: String,
    // IMDS returns these as strings.
    expires_in: Option<String>,
    expires_on: Option<String>,
}

/// Acquire an ARM token from IMDS. `client_id` selects a user-assigned identity; omit it for the
/// system-assigned identity.
pub async fn acquire(http: &reqwest::Client, client_id: Option<&str>) -> Result<TokenResponse> {
    let mut query = vec![("api-version", "2018-02-01"), ("resource", ARM_RESOURCE)];
    if let Some(cid) = client_id {
        query.push(("client_id", cid));
    }
    let resp = http
        .get(IMDS_TOKEN_URL)
        .header("Metadata", "true")
        .query(&query)
        .send()
        .await
        .map_err(|e| {
            RazError::Auth(format!(
                "IMDS unreachable — `--identity` only works on an Azure-hosted resource: {e}"
            ))
        })?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        return Err(RazError::Auth(format!(
            "IMDS token request failed ({status}): {}",
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
