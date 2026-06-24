//! OAuth 2.0 device authorization grant against Microsoft Entra.
//!
//! Flow: request a device code, show the user a URL + code to enter in a browser, then
//! poll the token endpoint until the user finishes (or the code expires). Uses the public
//! Azure CLI client id so no app registration is required, exactly like `az login`.

use serde::Deserialize;
use std::time::Duration;

use crate::error::{RazError, Result};

/// The Azure CLI's well-known public client id. Reused so device-code login works without
/// registering an application (same value az itself uses).
pub const CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";

/// Default scope: ARM management + the refresh-token/openid scopes.
pub const DEFAULT_SCOPE: &str =
    "https://management.azure.com/.default offline_access openid profile";

/// Microsoft Graph scope, for app/federated-credential management (`raz ad ...`).
pub const GRAPH_SCOPE: &str = "https://graph.microsoft.com/.default offline_access openid profile";

fn devicecode_url(tenant: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/devicecode")
}

pub(crate) fn token_url(tenant: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token")
}

/// Response from the devicecode endpoint. `message` already contains user-facing
/// instructions; front-ends can display it verbatim.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Verification URL with the code pre-filled, when the provider returns one.
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: i64,
    #[serde(default = "default_interval")]
    pub interval: u64,
    pub message: String,
}

fn default_interval() -> u64 {
    5
}

/// Open the device-login page in the user's default browser (best-effort — the URL and code are
/// still printed, so a failure or headless environment just falls back to manual entry). Prefers
/// the code-prefilled URL when present.
pub fn open_verification(dc: &DeviceCodeResponse) {
    let url = dc
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&dc.verification_uri);
    let _ = open::that(url);
}

/// Successful token response.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Lifetime in seconds from now.
    pub expires_in: i64,
}

/// Token endpoint error body (`error` is an OAuth error code such as
/// `authorization_pending`, `slow_down`, `expired_token`, `authorization_declined`).
#[derive(Debug, Clone, Deserialize)]
struct TokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Outcome of a single poll of the token endpoint.
pub enum PollOutcome {
    /// User hasn't completed yet — keep polling.
    Pending,
    /// Server asked us to slow down — increase the interval.
    SlowDown,
    /// Login succeeded.
    Granted(Box<TokenResponse>),
}

/// Step 1: request a device code for `tenant` (use "organizations" or "common" for the
/// default multi-tenant authority).
pub async fn request_device_code(
    http: &reqwest::Client,
    tenant: &str,
) -> Result<DeviceCodeResponse> {
    let resp = http
        .post(devicecode_url(tenant))
        .form(&[("client_id", CLIENT_ID), ("scope", DEFAULT_SCOPE)])
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(RazError::Auth(format!(
            "device code request failed: {body}"
        )));
    }
    Ok(resp.json::<DeviceCodeResponse>().await?)
}

/// Step 2 (single attempt): poll the token endpoint once.
pub async fn poll_token_once(
    http: &reqwest::Client,
    tenant: &str,
    device_code: &str,
) -> Result<PollOutcome> {
    let resp = http
        .post(token_url(tenant))
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", CLIENT_ID),
            ("device_code", device_code),
        ])
        .send()
        .await?;

    if resp.status().is_success() {
        let token = resp.json::<TokenResponse>().await?;
        return Ok(PollOutcome::Granted(Box::new(token)));
    }

    let err = resp.json::<TokenErrorResponse>().await?;
    match err.error.as_str() {
        "authorization_pending" => Ok(PollOutcome::Pending),
        "slow_down" => Ok(PollOutcome::SlowDown),
        "expired_token" | "code_expired" => Err(RazError::Auth(
            "device code expired before login completed".into(),
        )),
        "authorization_declined" => Err(RazError::Auth("login was declined".into())),
        other => Err(RazError::Auth(
            err.error_description.unwrap_or_else(|| other.to_string()),
        )),
    }
}

/// Exchange a refresh token for a fresh access token for `tenant` and `scope`. This is how raz
/// (like az) obtains per-tenant tokens after a single interactive login: resources are
/// tenant-scoped, so each tenant/scope needs its own token, redeemed from the login refresh
/// token. `scope` selects the audience (ARM vs Microsoft Graph).
pub async fn exchange_refresh_token(
    http: &reqwest::Client,
    tenant: &str,
    refresh_token: &str,
    scope: &str,
) -> Result<TokenResponse> {
    let resp = http
        .post(token_url(tenant))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", refresh_token),
            ("scope", scope),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(RazError::Auth(format!(
            "refresh-token exchange for tenant {tenant} failed: {body}"
        )));
    }
    Ok(resp.json::<TokenResponse>().await?)
}

/// Drive the whole flow to completion, blocking until the user finishes or the code
/// expires. `on_prompt` is called once with the device-code details so the caller can show
/// the user the URL/code (CLI prints it; TUI renders a panel).
pub async fn run_flow(
    http: &reqwest::Client,
    tenant: &str,
    on_prompt: impl FnOnce(&DeviceCodeResponse),
) -> Result<TokenResponse> {
    let dc = request_device_code(http, tenant).await?;
    on_prompt(&dc);

    let mut interval = dc.interval.max(1);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        match poll_token_once(http, tenant, &dc.device_code).await? {
            PollOutcome::Pending => {}
            PollOutcome::SlowDown => interval += 5,
            PollOutcome::Granted(token) => return Ok(*token),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_urls_include_tenant() {
        assert_eq!(
            devicecode_url("organizations"),
            "https://login.microsoftonline.com/organizations/oauth2/v2.0/devicecode"
        );
        assert!(token_url("common").ends_with("/common/oauth2/v2.0/token"));
    }

    #[test]
    fn device_code_response_parses_with_default_interval() {
        let json = r#"{
            "device_code": "DC",
            "user_code": "ABC-DEF",
            "verification_uri": "https://microsoft.com/devicelogin",
            "expires_in": 900,
            "message": "go here"
        }"#;
        let dc: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(dc.user_code, "ABC-DEF");
        assert_eq!(dc.interval, 5);
    }

    #[test]
    fn token_response_parses() {
        let json = r#"{"access_token":"tok","refresh_token":"r","expires_in":3600}"#;
        let t: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(t.access_token, "tok");
        assert_eq!(t.expires_in, 3600);
    }
}
