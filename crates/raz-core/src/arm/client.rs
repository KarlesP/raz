//! Thin ARM REST client over reqwest.
//!
//! Every call attaches the cached bearer token and targets the public ARM endpoint. HTTP
//! status is mapped onto [`RazError`] so commands inherit az-compatible exit codes
//! (404 -> NotFound -> exit 3, 401 -> NotLoggedIn, etc.).

use serde_json::Value;
use std::time::Duration;

use crate::auth::device_code::{self, TokenResponse};
use crate::config::Subscription;
use crate::error::{RazError, Result};

/// ARM endpoint host. Single-cloud only (public cloud); multi-cloud is out of scope.
const ARM_ENDPOINT: &str = "https://management.azure.com";

/// Default Azure region for resources raz creates.
pub const DEFAULT_LOCATION: &str = "westeurope";

/// API version for `Microsoft.Resources` operations (resource groups, provider registration).
const RESOURCES_API: &str = "2021-04-01";

/// Long-running-operation polling: interval and overall budget.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const POLL_MAX_ATTEMPTS: u32 = 240; // 240 * 5s = 20 min ceiling

/// Map an ARM HTTP status onto a [`RazError`], preserving az-compatible exit codes.
fn map_status(status: u16, path: &str, body: String) -> RazError {
    match status {
        401 => RazError::NotLoggedIn,
        403 => RazError::Auth(format!("forbidden: {body}")),
        404 => RazError::NotFound(path.to_string()),
        _ => RazError::Http(format!("ARM {status}: {body}")),
    }
}

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
            let Ok(tok) = device_code::exchange_refresh_token(
                http,
                &tenant.id,
                refresh,
                device_code::DEFAULT_SCOPE,
            )
            .await
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
    /// Build a client bound to a specific bearer token. Tokens are tenant-scoped, so callers
    /// mint the right one per subscription (see [`crate::context::Context`]).
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
        Err(map_status(status.as_u16(), path, body))
    }

    /// PUT a resource body to `path` at `api_version` (create or update). Returns the parsed
    /// response body. ARM creates/updates are long-running, so callers typically follow this
    /// with [`ArmClient::wait_provisioning`].
    pub async fn put(&self, path: &str, api_version: &str, body: &Value) -> Result<Value> {
        let url = format!("{ARM_ENDPOINT}{path}?api-version={api_version}");
        let resp = self
            .http
            .put(&url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Ok(serde_json::from_str(&text).unwrap_or(Value::Null));
        }
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status.as_u16(), path, text))
    }

    /// DELETE the resource at `path`. ARM deletes are long-running (202 Accepted), so callers
    /// follow with [`ArmClient::wait_deleted`]. A 404 is treated as already-gone (success).
    pub async fn delete(&self, path: &str, api_version: &str) -> Result<()> {
        let url = format!("{ARM_ENDPOINT}{path}?api-version={api_version}");
        let resp = self
            .http
            .delete(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;
        let status = resp.status();
        if status.is_success() || status.as_u16() == 404 {
            return Ok(());
        }
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status.as_u16(), path, text))
    }

    /// Poll a resource until its `properties.provisioningState` is terminal. Returns the final
    /// resource on `Succeeded`, errors on `Failed`/`Canceled` or timeout.
    pub async fn wait_provisioning(&self, path: &str, api_version: &str) -> Result<Value> {
        for _ in 0..POLL_MAX_ATTEMPTS {
            let resource = self.get(path, api_version).await?;
            let state = resource
                .get("properties")
                .and_then(|p| p.get("provisioningState"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match state {
                "Succeeded" => return Ok(resource),
                "Failed" | "Canceled" => {
                    return Err(RazError::Http(format!("provisioning {state} for {path}")))
                }
                _ => tokio::time::sleep(POLL_INTERVAL).await,
            }
        }
        Err(RazError::Http(format!(
            "timed out waiting for {path} to provision"
        )))
    }

    /// Poll until the resource at `path` no longer exists (delete completed).
    pub async fn wait_deleted(&self, path: &str, api_version: &str) -> Result<()> {
        for _ in 0..POLL_MAX_ATTEMPTS {
            match self.get(path, api_version).await {
                Err(RazError::NotFound(_)) => return Ok(()),
                Ok(_) => tokio::time::sleep(POLL_INTERVAL).await,
                Err(e) => return Err(e),
            }
        }
        Err(RazError::Http(format!(
            "timed out waiting for {path} to delete"
        )))
    }

    /// Ensure a resource group exists in `location` (idempotent PUT). Resource-group PUT is not
    /// long-running. Returns the resource group resource.
    pub async fn ensure_resource_group(
        &self,
        subscription: &str,
        resource_group: &str,
        location: &str,
    ) -> Result<Value> {
        let path = format!("/subscriptions/{subscription}/resourcegroups/{resource_group}");
        let body = serde_json::json!({ "location": location });
        self.put(&path, RESOURCES_API, &body).await
    }

    /// Ensure a resource-provider namespace (e.g. `Microsoft.Network`) is registered on the
    /// subscription, registering and polling to completion if needed. This mirrors what `az`
    /// does transparently on first use, so `raz` create commands work on fresh subscriptions.
    pub async fn ensure_provider_registered(
        &self,
        subscription: &str,
        namespace: &str,
    ) -> Result<()> {
        let path = format!("/subscriptions/{subscription}/providers/{namespace}");
        if self.provider_state(&path).await? == "Registered" {
            return Ok(());
        }

        // POST .../register (empty body — ARM requires an explicit zero Content-Length).
        let url = format!("{ARM_ENDPOINT}{path}/register?api-version={RESOURCES_API}");
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_LENGTH, 0)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_status(status.as_u16(), &path, body));
        }

        for _ in 0..POLL_MAX_ATTEMPTS {
            if self.provider_state(&path).await? == "Registered" {
                return Ok(());
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        Err(RazError::Http(format!(
            "timed out registering resource provider {namespace}"
        )))
    }

    async fn provider_state(&self, path: &str) -> Result<String> {
        let resource = self.get(path, RESOURCES_API).await?;
        Ok(resource
            .get("registrationState")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string())
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
