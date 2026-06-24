//! Subscription creation via `Microsoft.Subscription/aliases` (az `account subscription` /
//! `account alias`). Creating a subscription needs a billing scope + billing role (EA/MCA).

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2021-10-01";
const LOCATIONS_API: &str = "2022-12-01";
const MGMT_GROUPS_API: &str = "2020-05-01";

/// `account list-locations` — regions available to the subscription.
pub async fn list_locations(client: &ArmClient, subscription: &str) -> Result<Value> {
    let body = client
        .get(
            &format!("/subscriptions/{subscription}/locations"),
            LOCATIONS_API,
        )
        .await?;
    Ok(super::map_list(&body, |l| {
        json!({
            "name": l.get("name").and_then(Value::as_str).unwrap_or(""),
            "displayName": l.get("displayName").and_then(Value::as_str).unwrap_or(""),
            "regionalDisplayName": l.get("regionalDisplayName").and_then(Value::as_str).unwrap_or(""),
        })
    }))
}

/// `account management-group list` — management groups the identity can see (tenant scope).
pub async fn list_management_groups(client: &ArmClient) -> Result<Value> {
    let body = client
        .get(
            "/providers/Microsoft.Management/managementGroups",
            MGMT_GROUPS_API,
        )
        .await?;
    Ok(super::map_list(&body, |g| {
        json!({
            "name": g.get("name").and_then(Value::as_str).unwrap_or(""),
            "displayName": g.pointer("/properties/displayName").and_then(Value::as_str).unwrap_or(""),
        })
    }))
}

pub fn table_spec() -> TableSpec {
    vec![
        ("Alias", "name"),
        ("SubscriptionId", "subscriptionId"),
        ("State", "state"),
    ]
}

fn alias_path(alias: &str) -> String {
    format!("/providers/Microsoft.Subscription/aliases/{alias}")
}

fn flatten(body: &Value) -> Value {
    json!({
        "name": body.get("name").and_then(Value::as_str).unwrap_or(""),
        "subscriptionId": body.pointer("/properties/subscriptionId").and_then(Value::as_str).unwrap_or(""),
        "state": body.pointer("/properties/provisioningState").and_then(Value::as_str).unwrap_or(""),
    })
}

/// `raz subscription create` — request a new subscription under `billing_scope`.
// ponytail: provisioning is async; this returns the alias immediately. Use `subscription show`
// to poll the state rather than blocking here.
pub async fn create(
    client: &ArmClient,
    alias: &str,
    display_name: &str,
    billing_scope: &str,
    workload: &str,
) -> Result<Value> {
    let body = json!({
        "properties": {
            "displayName": display_name,
            "billingScope": billing_scope,
            "workload": workload,
        }
    });
    let resp = client.put(&alias_path(alias), API_VERSION, &body).await?;
    Ok(flatten(&resp))
}

/// `raz subscription show` — the alias and its provisioning state.
pub async fn show(client: &ArmClient, alias: &str) -> Result<Value> {
    let body = client.get(&alias_path(alias), API_VERSION).await?;
    Ok(flatten(&body))
}

/// `raz subscription delete` — remove the create-alias record. (Does not cancel the subscription
/// itself; the subscription, once created, persists.)
pub async fn delete_alias(client: &ArmClient, alias: &str) -> Result<()> {
    client.delete(&alias_path(alias), API_VERSION).await
}

/// `raz subscription rename` — change a subscription's display name.
pub async fn rename(client: &ArmClient, subscription_id: &str, name: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription_id}/providers/Microsoft.Subscription/rename");
    client
        .post(
            &path,
            API_VERSION,
            Some(&json!({ "subscriptionName": name })),
        )
        .await
}
