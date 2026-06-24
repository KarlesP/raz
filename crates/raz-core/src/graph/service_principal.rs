//! Service-principal creation for `raz ad sp create-for-rbac`: app registration + service
//! principal + client secret, via Microsoft Graph. The RBAC role assignment is done by the
//! command layer (ARM), mirroring `az ad sp create-for-rbac`.

use serde_json::{json, Value};

use super::client::GraphClient;
use crate::error::{RazError, Result};

fn field(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| RazError::Other(format!("Graph response missing '{key}'")))
}

/// Create an app registration, a service principal for it, and a client secret. Returns
/// `{ appId, objectId, password }` (objectId is the service principal's object id, used as the
/// RBAC principal id).
pub async fn create_with_secret(graph: &GraphClient, display_name: &str) -> Result<Value> {
    let app = graph
        .post("/applications", &json!({ "displayName": display_name }))
        .await?;
    let app_id = field(&app, "appId")?;
    let app_object_id = field(&app, "id")?;

    let sp = graph
        .post("/servicePrincipals", &json!({ "appId": app_id }))
        .await?;
    let sp_object_id = field(&sp, "id")?;

    let secret = graph
        .post(
            &format!("/applications/{app_object_id}/addPassword"),
            &json!({ "passwordCredential": { "displayName": "raz" } }),
        )
        .await?;
    let password = field(&secret, "secretText")?;

    Ok(json!({ "appId": app_id, "objectId": sp_object_id, "password": password }))
}
