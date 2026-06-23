//! Resource-group operations — the raz analogue of az's `group` command group.
//!
//! `create` is an idempotent PUT (not long-running); `delete` is a long-running operation that
//! cascades to every resource in the group, so it polls until the group is gone.

use serde_json::Value;

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2021-04-01";

fn group_path(subscription: &str, name: &str) -> String {
    format!("/subscriptions/{subscription}/resourcegroups/{name}")
}

/// Columns shown in `--output table`.
pub fn table_spec() -> TableSpec {
    TableSpec::new(vec![("Name", "name"), ("Location", "location")])
}

/// `raz group create -n <name> -l <location>` — create (or update) a resource group.
pub async fn create(
    client: &ArmClient,
    subscription: &str,
    name: &str,
    location: &str,
) -> Result<Value> {
    client
        .ensure_resource_group(subscription, name, location)
        .await
}

/// `raz group show -n <name>`.
pub async fn show(client: &ArmClient, subscription: &str, name: &str) -> Result<Value> {
    client
        .get(&group_path(subscription, name), API_VERSION)
        .await
}

/// `raz group list` — all resource groups in the subscription.
pub async fn list(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/resourcegroups");
    let body = client.get(&path, API_VERSION).await?;
    let items = body
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(Value::Array(items))
}

/// `raz group delete -n <name>` — delete a resource group and everything in it, waiting for
/// the operation to complete.
pub async fn delete(client: &ArmClient, subscription: &str, name: &str) -> Result<()> {
    let path = group_path(subscription, name);
    client.delete(&path, API_VERSION).await?;
    client.wait_deleted(&path, API_VERSION).await
}
