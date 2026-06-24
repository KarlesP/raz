//! ARM template deployments at resource-group scope (az `deployment group`). `create` deploys a
//! template (JSON; Bicep is compiled in the CLI layer) and waits; `what_if` previews changes.

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const DEPLOY_API: &str = "2021-04-01";

pub fn create_table_spec() -> TableSpec {
    vec![("Name", "name"), ("State", "provisioningState")]
}

pub fn whatif_table_spec() -> TableSpec {
    vec![("Change", "changeType"), ("Resource", "resourceId")]
}

fn deployment_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Resources/deployments/{name}"
    )
}

fn body(template: Value, parameters: Value, mode: &str) -> Value {
    json!({ "properties": { "mode": mode, "template": template, "parameters": parameters } })
}

/// `raz deployment group create` — deploy and wait for completion.
pub async fn create(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    template: Value,
    parameters: Value,
    mode: &str,
) -> Result<Value> {
    let path = deployment_path(subscription, resource_group, name);
    client
        .put(&path, DEPLOY_API, &body(template, parameters, mode))
        .await?;
    let done = client.wait_provisioning(&path, DEPLOY_API).await?;
    Ok(json!({
        "name": done.get("name").and_then(Value::as_str).unwrap_or(name),
        "provisioningState": done.pointer("/properties/provisioningState").and_then(Value::as_str).unwrap_or(""),
        "outputs": done.pointer("/properties/outputs").cloned().unwrap_or(Value::Null),
    }))
}

/// `raz deployment group what-if` — preview the changes without applying them.
pub async fn what_if(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    template: Value,
    parameters: Value,
    mode: &str,
) -> Result<Value> {
    let path = format!(
        "{}/whatIf",
        deployment_path(subscription, resource_group, name)
    );
    let result = client
        .post_and_wait_result(&path, DEPLOY_API, &body(template, parameters, mode))
        .await?;
    let changes = result
        .pointer("/properties/changes")
        .or_else(|| result.get("changes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rows = changes
        .iter()
        .map(|c| {
            json!({
                "changeType": c.get("changeType").and_then(Value::as_str).unwrap_or(""),
                "resourceId": c.get("resourceId").and_then(Value::as_str).unwrap_or(""),
            })
        })
        .collect();
    Ok(Value::Array(rows))
}
