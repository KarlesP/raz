//! Virtual network operations — the raz analogue of az's `network vnet` command group.
//!
//! `list`/`show` are live ARM reads; `create`/`delete` are intentionally stubbed in this
//! skeleton (they would issue PUT/DELETE and, for create, poll a long-running operation).

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2023-09-01";
const PROVIDER: &str = "Microsoft.Network/virtualNetworks";

fn resource_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/{PROVIDER}/{name}"
    )
}

/// Inputs for [`create`]. Defaults (location, prefixes, subnet) are supplied by the caller.
pub struct VnetCreate<'a> {
    pub subscription: &'a str,
    pub resource_group: &'a str,
    pub name: &'a str,
    pub location: &'a str,
    pub address_prefix: &'a str,
    pub subnet_name: &'a str,
    pub subnet_prefix: &'a str,
}

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
    let path = resource_path(subscription, resource_group, name);
    let mut body = client.get(&path, API_VERSION).await?;
    super::enrich_resource(&mut body);
    Ok(body)
}

/// `raz vnet create` — create a virtual network (defaults to West Europe) with a single
/// `default` subnet, then wait for provisioning to complete. The resource group is created if
/// it does not already exist.
pub async fn create(client: &ArmClient, args: &VnetCreate<'_>) -> Result<Value> {
    client
        .ensure_provider_registered(args.subscription, "Microsoft.Network")
        .await?;
    client
        .ensure_resource_group(args.subscription, args.resource_group, args.location)
        .await?;

    let path = resource_path(args.subscription, args.resource_group, args.name);
    let body = json!({
        "location": args.location,
        "properties": {
            "addressSpace": { "addressPrefixes": [args.address_prefix] },
            "subnets": [
                { "name": args.subnet_name, "properties": { "addressPrefix": args.subnet_prefix } }
            ]
        }
    });
    client.put(&path, API_VERSION, &body).await?;
    let mut final_state = client.wait_provisioning(&path, API_VERSION).await?;
    super::enrich_resource(&mut final_state);
    Ok(final_state)
}

/// `raz vnet update` — patch an existing virtual network's tags and/or append an address
/// prefix (read-modify-write), then wait for provisioning.
pub async fn update(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    tags: &[(String, String)],
    add_prefix: Option<&str>,
) -> Result<Value> {
    let path = resource_path(subscription, resource_group, name);
    let mut resource = client.get(&path, API_VERSION).await?;

    if !tags.is_empty() {
        let map = resource
            .as_object_mut()
            .expect("vnet resource is an object")
            .entry("tags")
            .or_insert_with(|| json!({}));
        if let Some(obj) = map.as_object_mut() {
            for (k, v) in tags {
                obj.insert(k.clone(), Value::String(v.clone()));
            }
        }
    }

    if let Some(prefix) = add_prefix {
        if let Some(arr) = resource
            .pointer_mut("/properties/addressSpace/addressPrefixes")
            .and_then(Value::as_array_mut)
        {
            arr.push(Value::String(prefix.to_string()));
        }
    }

    // Strip read-only fields that ARM rejects on write-back.
    if let Some(props) = resource
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        props.remove("provisioningState");
        props.remove("resourceGuid");
    }

    client.put(&path, API_VERSION, &resource).await?;
    let mut final_state = client.wait_provisioning(&path, API_VERSION).await?;
    super::enrich_resource(&mut final_state);
    Ok(final_state)
}

/// `raz vnet delete` — delete a virtual network and wait for the operation to finish.
pub async fn delete(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<()> {
    let path = resource_path(subscription, resource_group, name);
    client.delete(&path, API_VERSION).await?;
    client.wait_deleted(&path, API_VERSION).await
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
