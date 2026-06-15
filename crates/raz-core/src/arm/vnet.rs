//! Virtual network operations — the raz analogue of az's `network vnet` command group.
//!
//! `list`/`show` are live ARM reads; `create`/`delete` are intentionally stubbed in this
//! skeleton (they would issue PUT/DELETE and, for create, poll a long-running operation).

use serde_json::Value;

use super::client::ArmClient;
use crate::error::{RazError, Result};
use crate::output::TableSpec;

const API_VERSION: &str = "2023-09-01";
const PROVIDER: &str = "Microsoft.Network/virtualNetworks";

/// Columns shown in `--output table`, matching az's vnet table shape.
pub fn table_spec() -> TableSpec {
    TableSpec::new(vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
    ])
}

/// `raz vnet list` — all virtual networks in the subscription.
pub async fn list(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/{PROVIDER}");
    let body = client.get(&path, API_VERSION).await?;
    Ok(normalize_list(body))
}

/// `raz vnet show -g <rg> -n <name>` — a single virtual network.
pub async fn show(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<Value> {
    let path = format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/{PROVIDER}/{name}"
    );
    let mut body = client.get(&path, API_VERSION).await?;
    super::enrich_resource(&mut body);
    Ok(body)
}

/// `raz vnet create` — stubbed.
pub async fn create(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented(
        "vnet create issues an ARM PUT and polls a long-running operation; not wired in this skeleton"
            .into(),
    ))
}

/// `raz vnet delete` — stubbed.
pub async fn delete(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented(
        "vnet delete issues an ARM DELETE (long-running); not wired in this skeleton".into(),
    ))
}

/// Flatten an ARM `{ "value": [...] }` list into an array, enriching each item with a
/// `resourceGroup` field parsed from its id (az surfaces this derived field too).
fn normalize_list(body: Value) -> Value {
    let mut items = body
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in &mut items {
        super::enrich_resource(item);
    }
    Value::Array(items)
}
