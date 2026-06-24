//! Key Vault **management plane** (az `keyvault list|show|create`) via ARM
//! `Microsoft.KeyVault/vaults`. Secret get/set is the data plane — see `commands::keyvault`.

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2023-07-01";
const PROVIDER: &str = "Microsoft.KeyVault/vaults";

pub fn table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
        ("Uri", "uri"),
    ]
}

fn vault_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/{PROVIDER}/{name}"
    )
}

fn flatten(mut v: Value) -> Value {
    super::enrich_resource(&mut v); // derives resourceGroup from the id
    json!({
        "name": v.get("name").and_then(Value::as_str).unwrap_or(""),
        "resourceGroup": v.get("resourceGroup").and_then(Value::as_str).unwrap_or(""),
        "location": v.get("location").and_then(Value::as_str).unwrap_or(""),
        "uri": v.pointer("/properties/vaultUri").and_then(Value::as_str).unwrap_or(""),
    })
}

/// `raz keyvault list`.
pub async fn list(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/{PROVIDER}");
    let body = client.get(&path, API_VERSION).await?;
    let items = body
        .get("value")
        .and_then(Value::as_array)
        .map(|v| v.iter().cloned().map(flatten).collect())
        .unwrap_or_default();
    Ok(Value::Array(items))
}

/// `raz keyvault show`.
pub async fn show(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<Value> {
    let body = client
        .get(&vault_path(subscription, resource_group, name), API_VERSION)
        .await?;
    Ok(flatten(body))
}

/// `raz keyvault create` — RBAC-authorization vault (no access policies). `tenant_id` is the
/// vault's Entra tenant; register the provider, PUT, and wait for provisioning.
pub async fn create(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    location: &str,
    tenant_id: &str,
    sku: &str,
) -> Result<Value> {
    client
        .ensure_provider_registered(subscription, "Microsoft.KeyVault")
        .await?;
    let path = vault_path(subscription, resource_group, name);
    let body = json!({
        "location": location,
        "properties": {
            "tenantId": tenant_id,
            "sku": { "family": "A", "name": sku },
            "enableRbacAuthorization": true,
            "accessPolicies": [],
        }
    });
    client.put(&path, API_VERSION, &body).await?;
    let done = client.wait_provisioning(&path, API_VERSION).await?;
    Ok(flatten(done))
}
