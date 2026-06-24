//! Generic resource operations (az `resource`) — the escape hatch for any resource type by id.
//! `list` uses the fixed Resources api-version; `show`/`delete` take a resource id and require
//! the resource's own `--api-version` (raz doesn't track per-type versions).

use serde_json::Value;

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

/// API version for the generic resources list endpoint.
const LIST_API: &str = "2021-04-01";

pub fn table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("Type", "type"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
    ]
}

/// `resource list` — all resources in the subscription or a resource group, optionally filtered
/// by `resourceType` (e.g. `Microsoft.Compute/virtualMachines`).
pub async fn list(
    client: &ArmClient,
    subscription: &str,
    resource_group: Option<&str>,
    resource_type: Option<&str>,
) -> Result<Value> {
    let base = match resource_group {
        Some(rg) => format!("/subscriptions/{subscription}/resourceGroups/{rg}/resources"),
        None => format!("/subscriptions/{subscription}/resources"),
    };
    let path = match resource_type {
        Some(t) => format!("{base}?$filter=resourceType%20eq%20%27{t}%27"),
        None => base,
    };
    let body = client.get(&path, LIST_API).await?;
    Ok(super::enrich_list(body))
}

/// `resource show --ids <id> --api-version <v>`.
pub async fn show(client: &ArmClient, id: &str, api_version: &str) -> Result<Value> {
    let mut body = client.get(id, api_version).await?;
    super::enrich_resource(&mut body);
    Ok(body)
}

/// `resource delete --ids <id> --api-version <v>` — DELETE and wait for removal.
pub async fn delete(client: &ArmClient, id: &str, api_version: &str) -> Result<()> {
    client.delete(id, api_version).await?;
    client.wait_deleted(id, api_version).await
}
