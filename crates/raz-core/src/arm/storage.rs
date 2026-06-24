//! Storage accounts and blob containers via the ARM management plane (az `storage account` /
//! `storage container`). Management-plane only — no data-plane keys required.

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2023-01-01";
const PROVIDER: &str = "Microsoft.Storage/storageAccounts";

pub fn account_table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
        ("Sku", "sku"),
        ("Kind", "kind"),
    ]
}

pub fn container_table_spec() -> TableSpec {
    vec![("Name", "name"), ("PublicAccess", "publicAccess")]
}

fn account_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/{PROVIDER}/{name}"
    )
}

fn flatten_account(v: &Value) -> Value {
    let mut v = v.clone();
    super::enrich_resource(&mut v); // derives resourceGroup from the id
    json!({
        "name": v.get("name").and_then(Value::as_str).unwrap_or(""),
        "resourceGroup": v.get("resourceGroup").and_then(Value::as_str).unwrap_or(""),
        "location": v.get("location").and_then(Value::as_str).unwrap_or(""),
        "sku": v.pointer("/sku/name").and_then(Value::as_str).unwrap_or(""),
        "kind": v.get("kind").and_then(Value::as_str).unwrap_or(""),
        "provisioningState": v.pointer("/properties/provisioningState").and_then(Value::as_str).unwrap_or(""),
    })
}

/// `raz storage account list`.
pub async fn list_accounts(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/{PROVIDER}");
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::map_list(&body, flatten_account))
}

/// `raz storage account show`.
pub async fn show_account(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<Value> {
    let body = client
        .get(
            &account_path(subscription, resource_group, name),
            API_VERSION,
        )
        .await?;
    Ok(flatten_account(&body))
}

/// `raz storage account create` — register the provider, PUT, and wait for provisioning.
pub async fn create_account(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    location: &str,
    sku: &str,
    kind: &str,
) -> Result<Value> {
    client
        .ensure_provider_registered(subscription, "Microsoft.Storage")
        .await?;
    let path = account_path(subscription, resource_group, name);
    let body = json!({ "location": location, "sku": { "name": sku }, "kind": kind });
    client.put(&path, API_VERSION, &body).await?;
    let done = client.wait_provisioning(&path, API_VERSION).await?;
    Ok(flatten_account(&done))
}

fn containers_path(subscription: &str, resource_group: &str, account: &str) -> String {
    format!(
        "{}/blobServices/default/containers",
        account_path(subscription, resource_group, account)
    )
}

fn flatten_container(v: &Value) -> Value {
    json!({
        "name": v.get("name").and_then(Value::as_str).unwrap_or(""),
        "publicAccess": v.pointer("/properties/publicAccess").and_then(Value::as_str).unwrap_or("None"),
    })
}

/// `raz storage container list`.
pub async fn list_containers(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    account: &str,
) -> Result<Value> {
    let body = client
        .get(
            &containers_path(subscription, resource_group, account),
            API_VERSION,
        )
        .await?;
    Ok(super::map_list(&body, flatten_container))
}

/// `raz storage container create`.
pub async fn create_container(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    account: &str,
    name: &str,
) -> Result<Value> {
    let path = format!(
        "{}/{name}",
        containers_path(subscription, resource_group, account)
    );
    let body = client.put(&path, API_VERSION, &json!({})).await?;
    Ok(flatten_container(&body))
}
