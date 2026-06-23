//! Federated identity credentials on an Entra app registration — the GitHub<->Entra OIDC trust.
//! Mirrors `az ad app federated-credential {create,list,delete}`.

use serde_json::{json, Map, Value};

use super::client::GraphClient;
use crate::error::{RazError, Result};
use crate::output::TableSpec;

/// Columns for `--output table`.
pub fn table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("Subject", "subject"),
        ("Issuer", "issuer"),
    ]
}

/// Resolve an app's directory **object id** from either its appId (client id) or object id.
/// az's `--id` accepts both; Graph addresses applications by object id.
async fn resolve_object_id(client: &GraphClient, id: &str) -> Result<String> {
    // $filter must be URL-encoded (space -> %20, quote -> %27); an appId is a plain GUID.
    let path = format!("/applications?$filter=appId%20eq%20%27{id}%27&$select=id");
    if let Ok(body) = client.get(&path).await {
        if let Some(obj) = body
            .get("value")
            .and_then(Value::as_array)
            .and_then(|v| v.first())
            .and_then(|a| a.get("id"))
            .and_then(Value::as_str)
        {
            return Ok(obj.to_string());
        }
    }
    // Not found by appId — assume the caller passed the object id directly.
    Ok(id.to_string())
}

/// `create` — add a federated identity credential to the app registration.
#[allow(clippy::too_many_arguments)]
pub async fn create(
    client: &GraphClient,
    app: &str,
    name: &str,
    issuer: &str,
    subject: &str,
    audience: &str,
    description: Option<&str>,
) -> Result<Value> {
    let obj = resolve_object_id(client, app).await?;
    let mut body = Map::new();
    body.insert("name".into(), json!(name));
    body.insert("issuer".into(), json!(issuer));
    body.insert("subject".into(), json!(subject));
    body.insert("audiences".into(), json!([audience]));
    if let Some(d) = description {
        body.insert("description".into(), json!(d));
    }
    client
        .post(
            &format!("/applications/{obj}/federatedIdentityCredentials"),
            &Value::Object(body),
        )
        .await
}

/// `list` — the app registration's federated identity credentials.
pub async fn list(client: &GraphClient, app: &str) -> Result<Value> {
    let obj = resolve_object_id(client, app).await?;
    let body = client
        .get(&format!("/applications/{obj}/federatedIdentityCredentials"))
        .await?;
    Ok(body
        .get("value")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new())))
}

/// `delete` — remove a federated identity credential by its `name` (resolved to its id).
pub async fn delete(client: &GraphClient, app: &str, name: &str) -> Result<()> {
    let obj = resolve_object_id(client, app).await?;
    let creds = client
        .get(&format!("/applications/{obj}/federatedIdentityCredentials"))
        .await?;
    let id = creds
        .get("value")
        .and_then(Value::as_array)
        .and_then(|v| {
            v.iter()
                .find(|c| c.get("name").and_then(Value::as_str) == Some(name))
        })
        .and_then(|c| c.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| RazError::NotFound(format!("federated credential '{name}'")))?
        .to_string();
    client
        .delete(&format!(
            "/applications/{obj}/federatedIdentityCredentials/{id}"
        ))
        .await
}
