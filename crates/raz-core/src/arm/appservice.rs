//! App Service (az `webapp` / `appservice plan`) via ARM `Microsoft.Web`. Web PUTs are
//! synchronous (no LRO polling). `create` wires a site to an existing plan.

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2023-12-01";

pub fn webapp_table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
        ("HostName", "defaultHostName"),
        ("State", "state"),
    ]
}

pub fn plan_table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
        ("Sku", "sku"),
        ("Kind", "kind"),
    ]
}

fn site_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!("/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Web/sites/{name}")
}

fn plan_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!("/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Web/serverfarms/{name}")
}

fn flatten_site(v: &Value) -> Value {
    let mut v = v.clone();
    super::enrich_resource(&mut v);
    json!({
        "name": v.get("name").and_then(Value::as_str).unwrap_or(""),
        "resourceGroup": v.get("resourceGroup").and_then(Value::as_str).unwrap_or(""),
        "location": v.get("location").and_then(Value::as_str).unwrap_or(""),
        "defaultHostName": v.pointer("/properties/defaultHostName").and_then(Value::as_str).unwrap_or(""),
        "state": v.pointer("/properties/state").and_then(Value::as_str).unwrap_or(""),
    })
}

fn flatten_plan(v: &Value) -> Value {
    let mut v = v.clone();
    super::enrich_resource(&mut v);
    json!({
        "name": v.get("name").and_then(Value::as_str).unwrap_or(""),
        "resourceGroup": v.get("resourceGroup").and_then(Value::as_str).unwrap_or(""),
        "location": v.get("location").and_then(Value::as_str).unwrap_or(""),
        "sku": v.pointer("/sku/name").and_then(Value::as_str).unwrap_or(""),
        "kind": v.get("kind").and_then(Value::as_str).unwrap_or(""),
    })
}

/// `raz webapp list`.
pub async fn list_webapps(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/Microsoft.Web/sites");
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::map_list(&body, flatten_site))
}

/// `raz webapp show`.
pub async fn show_webapp(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<Value> {
    let body = client
        .get(&site_path(subscription, resource_group, name), API_VERSION)
        .await?;
    Ok(flatten_site(&body))
}

/// `raz webapp create` — create a site bound to existing plan `plan` (which supplies the region).
pub async fn create_webapp(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    plan: &str,
    runtime: Option<&str>,
) -> Result<Value> {
    client
        .ensure_provider_registered(subscription, "Microsoft.Web")
        .await?;
    let plan_id = plan_path(subscription, resource_group, plan);
    // The site must live in the plan's region; fetch it from the plan (errors if the plan is missing).
    let plan_body = client.get(&plan_id, API_VERSION).await?;
    let location = plan_body
        .get("location")
        .and_then(Value::as_str)
        .unwrap_or("");

    let mut properties = json!({ "serverFarmId": plan_id });
    if let Some(rt) = runtime {
        properties["siteConfig"] = json!({ "linuxFxVersion": rt });
    }
    let body = json!({ "location": location, "properties": properties });
    let site = client
        .put(
            &site_path(subscription, resource_group, name),
            API_VERSION,
            &body,
        )
        .await?;
    Ok(flatten_site(&site))
}

/// `raz appservice plan create` — App Service plan (Linux when `linux` is set).
pub async fn create_plan(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    location: &str,
    sku: &str,
    linux: bool,
) -> Result<Value> {
    client
        .ensure_provider_registered(subscription, "Microsoft.Web")
        .await?;
    let body = json!({
        "location": location,
        "sku": { "name": sku },
        "kind": if linux { "linux" } else { "app" },
        "properties": { "reserved": linux },
    });
    let plan = client
        .put(
            &plan_path(subscription, resource_group, name),
            API_VERSION,
            &body,
        )
        .await?;
    Ok(flatten_plan(&plan))
}
