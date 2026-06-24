//! Resource tags via the `Microsoft.Resources/tags/default` API (az `tag`). Works on any ARM
//! scope: a resource id, a resource group, or a subscription.

use serde_json::{json, Map, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2021-04-01";

pub fn table_spec() -> TableSpec {
    vec![("Tag", "tag"), ("Value", "value")]
}

fn tags_path(resource_id: &str) -> String {
    format!("{resource_id}/providers/Microsoft.Resources/tags/default")
}

/// Read the current tag map at `resource_id` (empty if none).
async fn fetch(client: &ArmClient, resource_id: &str) -> Result<Map<String, Value>> {
    let body = client.get(&tags_path(resource_id), API_VERSION).await?;
    Ok(body
        .pointer("/properties/tags")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}

/// Render a tag map as sorted `{tag, value}` rows for output.
fn rows(tags: &Map<String, Value>) -> Value {
    let mut pairs: Vec<(&String, &Value)> = tags.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    Value::Array(
        pairs
            .into_iter()
            .map(|(k, v)| json!({ "tag": k, "value": v.as_str().unwrap_or("") }))
            .collect(),
    )
}

/// Replace the tag map at `resource_id`, returning the resulting rows.
async fn put(client: &ArmClient, resource_id: &str, tags: Map<String, Value>) -> Result<Value> {
    let body = client
        .put(
            &tags_path(resource_id),
            API_VERSION,
            &json!({ "properties": { "tags": tags } }),
        )
        .await?;
    let tags = body
        .pointer("/properties/tags")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    Ok(rows(&tags))
}

/// `raz tag list --resource <id>`.
pub async fn list(client: &ArmClient, resource_id: &str) -> Result<Value> {
    Ok(rows(&fetch(client, resource_id).await?))
}

/// `raz tag add` — merge `pairs` into the existing tags (read-modify-write).
// ponytail: GET+PUT read-modify-write, not atomic; fine for interactive use — switch to PATCH
// Merge if concurrent writers matter.
pub async fn add(
    client: &ArmClient,
    resource_id: &str,
    pairs: &[(String, String)],
) -> Result<Value> {
    let mut tags = fetch(client, resource_id).await?;
    for (k, v) in pairs {
        tags.insert(k.clone(), Value::String(v.clone()));
    }
    put(client, resource_id, tags).await
}

/// `raz tag delete` — drop the named keys (read-modify-write).
pub async fn delete(client: &ArmClient, resource_id: &str, keys: &[String]) -> Result<Value> {
    let mut tags = fetch(client, resource_id).await?;
    for k in keys {
        tags.remove(k);
    }
    put(client, resource_id, tags).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_sorted_pairs() {
        let mut m = Map::new();
        m.insert("z".into(), Value::String("1".into()));
        m.insert("a".into(), Value::String("2".into()));
        let r = rows(&m);
        assert_eq!(r[0]["tag"], "a");
        assert_eq!(r[1]["tag"], "z");
    }
}
