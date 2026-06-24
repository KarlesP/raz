//! Management locks via `Microsoft.Authorization/locks` (az `lock`). Scope is a resource id, a
//! resource group, or the subscription. `level` is `CanNotDelete` or `ReadOnly`.

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2016-09-01";

pub fn table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("Level", "level"),
        ("Notes", "notes"),
        ("Scope", "scope"),
    ]
}

/// Resolve the lock scope: a resource id wins, else fall back to the subscription/RG scope.
pub fn scope(
    subscription: &str,
    resource_group: Option<&str>,
    resource_id: Option<&str>,
) -> String {
    match resource_id {
        Some(id) => id.to_string(),
        None => super::role::scope(subscription, resource_group),
    }
}

fn flatten(lock: &Value) -> Value {
    json!({
        "name": lock.get("name").and_then(Value::as_str).unwrap_or(""),
        "level": lock.pointer("/properties/level").and_then(Value::as_str).unwrap_or(""),
        "notes": lock.pointer("/properties/notes").and_then(Value::as_str).unwrap_or(""),
        "scope": lock.get("id").and_then(Value::as_str).unwrap_or(""),
    })
}

/// `raz lock create` — apply a lock named `name` at `scope`.
pub async fn create(
    client: &ArmClient,
    scope: &str,
    name: &str,
    level: &str,
    notes: Option<&str>,
) -> Result<Value> {
    let path = format!("{scope}/providers/Microsoft.Authorization/locks/{name}");
    let mut props = json!({ "level": level });
    if let Some(n) = notes {
        props["notes"] = json!(n);
    }
    let lock = client
        .put(&path, API_VERSION, &json!({ "properties": props }))
        .await?;
    Ok(flatten(&lock))
}

/// `raz lock list` — locks at `scope`.
pub async fn list(client: &ArmClient, scope: &str) -> Result<Value> {
    let path = format!("{scope}/providers/Microsoft.Authorization/locks");
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::map_list(&body, flatten))
}

/// `raz lock delete` — remove the lock named `name` at `scope`.
pub async fn delete(client: &ArmClient, scope: &str, name: &str) -> Result<()> {
    let path = format!("{scope}/providers/Microsoft.Authorization/locks/{name}");
    client.delete(&path, API_VERSION).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_tiers() {
        assert_eq!(scope("s", None, None), "/subscriptions/s");
        assert_eq!(
            scope("s", Some("rg"), None),
            "/subscriptions/s/resourceGroups/rg"
        );
        assert_eq!(scope("s", Some("rg"), Some("/some/id")), "/some/id");
    }
}
