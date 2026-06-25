//! Service-principal sign-in: the OAuth 2.0 `client_credentials` grant, with either a client
//! secret or an OIDC **federated token** (`client_assertion`), matching `az login
//! --service-principal`. Also fetches the GitHub Actions OIDC token, like the `azure/login`
//! action does, so CI can sign in without any secret.

use serde::Deserialize;

use super::device_code::{token_url, TokenResponse};
use crate::error::{RazError, Result};

/// OIDC client-assertion type for the federated-token grant.
const JWT_BEARER: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// Sign in as a service principal using a client secret. `authority` and `arm` are the active
/// cloud's AAD authority and ARM endpoint (the client-credentials scope is `{arm}/.default`).
pub async fn acquire_client_secret(
    http: &reqwest::Client,
    authority: &str,
    arm: &str,
    tenant: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<TokenResponse> {
    let scope = format!("{arm}/.default");
    let form = [
        ("grant_type", "client_credentials"),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("scope", scope.as_str()),
    ];
    post_token(http, authority, tenant, &form).await
}

/// Sign in as a service principal using an OIDC federated token (`client_assertion`). No secret
/// leaves the machine — the assertion is a short-lived JWT minted by a trusted IdP (e.g. GitHub
/// Actions), exchanged for an Azure token via the workload-identity federation configured on the
/// app registration.
pub async fn acquire_federated(
    http: &reqwest::Client,
    authority: &str,
    arm: &str,
    tenant: &str,
    client_id: &str,
    assertion: &str,
) -> Result<TokenResponse> {
    let scope = format!("{arm}/.default");
    let form = [
        ("grant_type", "client_credentials"),
        ("client_id", client_id),
        ("client_assertion_type", JWT_BEARER),
        ("client_assertion", assertion),
        ("scope", scope.as_str()),
    ];
    post_token(http, authority, tenant, &form).await
}

async fn post_token(
    http: &reqwest::Client,
    authority: &str,
    tenant: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse> {
    let resp = http
        .post(token_url(authority, tenant))
        .form(form)
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(RazError::Auth(format!(
            "service-principal sign-in failed: {body}"
        )));
    }
    Ok(resp.json::<TokenResponse>().await?)
}

/// Fetch an OIDC token from the GitHub Actions runtime, the same way the `azure/login` action
/// does: requires the workflow to grant `permissions: id-token: write`, which sets
/// `ACTIONS_ID_TOKEN_REQUEST_URL` and `ACTIONS_ID_TOKEN_REQUEST_TOKEN`. `audience` is the token
/// audience the app registration's federated credential expects (default
/// `api://AzureADTokenExchange`).
pub async fn fetch_github_oidc_token(http: &reqwest::Client, audience: &str) -> Result<String> {
    let url = std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL").map_err(|_| {
        RazError::Auth(
            "no --federated-token and not in a GitHub Actions OIDC context \
             (set permissions: id-token: write, or pass --federated-token)"
                .into(),
        )
    })?;
    let bearer = std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
        .map_err(|_| RazError::Auth("ACTIONS_ID_TOKEN_REQUEST_TOKEN is not set".into()))?;

    #[derive(Deserialize)]
    struct OidcToken {
        value: String,
    }

    let resp = http
        .get(format!("{url}&audience={audience}"))
        .bearer_auth(bearer)
        .send()
        .await?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(RazError::Auth(format!(
            "failed to fetch GitHub OIDC token: {body}"
        )));
    }
    Ok(resp.json::<OidcToken>().await?.value)
}
