//! Thin ARM REST client over reqwest.
//!
//! Every call attaches the cached bearer token and targets the public ARM endpoint. HTTP
//! status is mapped onto [`RazError`] so commands inherit az-compatible exit codes
//! (404 -> NotFound -> exit 3, 401 -> NotLoggedIn, etc.).

use serde_json::Value;

use crate::auth::device_code::{self, TokenResponse};
use crate::config::Subscription;
use crate::context::Context;
use crate::error::{RazError, Result};

/// ARM endpoint host. Single-cloud only (public cloud); multi-cloud is out of scope.
const ARM_ENDPOINT: &str = "https://management.azure.com";

/// API version used for subscription/tenant discovery during login.
const SUBSCRIPTIONS_API_VERSION: &str = "2022-12-01";

/// An Entra tenant the signed-in identity can access (from ARM `/tenants`).
#[derive(Debug, Clone)]
pub struct Tenant {
    pub id: String,
    pub display_name: String,
    pub default_domain: String,
}

/// Discover every tenant the identity can reach and the subscriptions in each, mirroring how
/// `az login` enumerates across tenants. Uses the initial login's refresh token to mint a
/// per-tenant ARM token silently (no extra prompts). Falls back to the home-tenant token if
/// no refresh token is available. The first subscription found is marked default.
pub async fn discover_all(
    http: &reqwest::Client,
    token: &TokenResponse,
) -> Result<(Vec<Tenant>, Vec<Subscription>)> {
    let home = ArmClient::with_token(http.clone(), token.access_token.clone());
    let tenants = home.list_tenants().await.unwrap_or_default();

    let mut subs: Vec<Subscription> = Vec::new();
    if let Some(refresh) = &token.refresh_token {
        for tenant in &tenants {
            let Ok(tok) = device_code::exchange_refresh_token(http, &tenant.id, refresh).await
            else {
                continue; // tenant not redeemable for this identity; skip it
            };
            let client = ArmClient::with_token(http.clone(), tok.access_token);
            if let Ok(mut found) = client.list_subscriptions().await {
                for sub in &mut found {
                    if sub.tenant_id.is_empty() {
                        sub.tenant_id = tenant.id.clone();
                    }
                }
                subs.append(&mut found);
            }
        }
    }

    // Fallback: if cross-tenant enumeration yielded nothing, use the home-tenant token.
    if subs.is_empty() {
        if let Ok(mut found) = home.list_subscriptions().await {
            subs.append(&mut found);
        }
    }

    for (idx, sub) in subs.iter_mut().enumerate() {
        sub.is_default = idx == 0;
    }
    Ok((tenants, subs))
}

pub struct ArmClient {
    http: reqwest::Client,
    token: String,
}

impl ArmClient {
    /// Build a client from the active [`Context`], requiring a valid cached token.
    pub fn from_context(ctx: &Context) -> Result<Self> {
        Ok(Self {
            http: ctx.http.clone(),
            token: ctx.access_token()?,
        })
    }

    /// Build directly from a token (used during login before a Context exists).
    pub fn with_token(http: reqwest::Client, token: String) -> Self {
        Self { http, token }
    }

    /// GET an ARM resource path (everything after the endpoint host) at `api_version`,
    /// returning the parsed JSON body.
    pub async fn get(&self, path: &str, api_version: &str) -> Result<Value> {
        let sep = if path.contains('?') { '&' } else { '?' };
        let url = format!("{ARM_ENDPOINT}{path}{sep}api-version={api_version}");
        let resp = self.http.get(&url).bearer_auth(&self.token).send().await?;

        let status = resp.status();
        if status.is_success() {
            return Ok(resp.json::<Value>().await?);
        }

        let body = resp.text().await.unwrap_or_default();
        Err(match status.as_u16() {
            401 => RazError::NotLoggedIn,
            403 => RazError::Auth(format!("forbidden: {body}")),
            404 => RazError::NotFound(path.to_string()),
            _ => RazError::Http(format!("ARM {status}: {body}")),
        })
    }

    /// Discover the subscriptions the signed-in identity can see. Used by `raz login`.
    pub async fn list_subscriptions(&self) -> Result<Vec<Subscription>> {
        let body = self
            .get("/subscriptions", SUBSCRIPTIONS_API_VERSION)
            .await?;
        let items = body
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut subs = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            let id = item
                .get("subscriptionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if id.is_empty() {
                continue;
            }
            subs.push(Subscription {
                name: item
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string(),
                tenant_id: item
                    .get("tenantId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                is_default: idx == 0,
                id,
            });
        }
        Ok(subs)
    }

    /// Discover the tenants the signed-in identity can access (ARM `/tenants`).
    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        let body = self.get("/tenants", SUBSCRIPTIONS_API_VERSION).await?;
        let items = body
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut tenants = Vec::new();
        for item in &items {
            let id = item
                .get("tenantId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if id.is_empty() {
                continue;
            }
            tenants.push(Tenant {
                display_name: item
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                default_domain: item
                    .get("defaultDomain")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                id,
            });
        }
        Ok(tenants)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_parsing_marks_first_default() {
        // Exercise the projection logic without a network call by reusing the same shape.
        let body = serde_json::json!({
            "value": [
                {"subscriptionId": "s1", "displayName": "Dev", "tenantId": "t1"},
                {"subscriptionId": "s2", "displayName": "Prod", "tenantId": "t1"},
                {"displayName": "no-id"}
            ]
        });
        let items = body["value"].as_array().unwrap();
        // Mirror list_subscriptions' projection inline.
        let mut subs = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            let id = item
                .get("subscriptionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if id.is_empty() {
                continue;
            }
            subs.push((id, idx == 0));
        }
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0], ("s1".to_string(), true));
        assert_eq!(subs[1], ("s2".to_string(), false));
    }
}
