//! Virtual machine operations — the raz analogue of az's `vm` command group.
//!
//! `list`/`show` are live ARM reads; `create`/`delete`/`start`/`stop` are stubbed. Real
//! start/stop are long-running operations (POST + poll), noted in each stub message.

use serde_json::Value;

use super::client::ArmClient;
use crate::error::{RazError, Result};
use crate::output::TableSpec;

const API_VERSION: &str = "2024-07-01";
const PROVIDER: &str = "Microsoft.Compute/virtualMachines";

/// Columns shown in `--output table`, matching az's vm table shape.
pub fn table_spec() -> TableSpec {
    TableSpec::new(vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
    ])
}

/// `raz vm list` — all virtual machines in the subscription.
pub async fn list(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/{PROVIDER}");
    let body = client.get(&path, API_VERSION).await?;
    Ok(normalize_list(body))
}

/// `raz vm show -g <rg> -n <name>` — a single virtual machine.
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

/// `raz vm create` — stubbed.
pub async fn create(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented(
        "vm create composes several ARM resources (nic, disk, vm) and polls an LRO; not wired in this skeleton"
            .into(),
    ))
}

/// `raz vm delete` — stubbed.
pub async fn delete(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented(
        "vm delete issues an ARM DELETE (long-running); not wired in this skeleton".into(),
    ))
}

/// `raz vm start` — stubbed (real impl POSTs `/start` and polls the LRO).
pub async fn start(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented(
        "vm start POSTs the /start action and polls a long-running operation; not wired in this skeleton"
            .into(),
    ))
}

/// `raz vm stop` — stubbed (real impl POSTs `/powerOff` and polls the LRO).
pub async fn stop(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented(
        "vm stop POSTs the /powerOff action and polls a long-running operation; not wired in this skeleton"
            .into(),
    ))
}

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
