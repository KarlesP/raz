//! Subscription creation via `Microsoft.Subscription/aliases` (az `account subscription` /
//! `account alias`). Creating a subscription needs a billing scope + billing role (EA/MCA).

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2021-10-01";

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
