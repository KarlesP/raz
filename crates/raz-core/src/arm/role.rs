//! Azure RBAC — role definitions and role assignments under `Microsoft.Authorization`.
//! Mirrors `az role definition list` and `az role assignment {list,create,delete}`.
//!
//! Scope is a subscription (`/subscriptions/{sub}`) or a resource group within it. Assignees are
//! given by **principal object id** (resolving UPN/appId via Graph is a later enhancement).

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::{RazError, Result};
use crate::output::TableSpec;

const API_VERSION: &str = "2022-04-01";

/// Build the ARM scope path: a resource group within `subscription`, else the subscription.
pub fn scope(subscription: &str, resource_group: Option<&str>) -> String {
    match resource_group {
        Some(rg) => format!("/subscriptions/{subscription}/resourceGroups/{rg}"),
        None => format!("/subscriptions/{subscription}"),
    }
}

pub fn definition_table() -> TableSpec {
    vec![("Role", "roleName"), ("Id", "name"), ("Type", "type")]
}

pub fn assignment_table() -> TableSpec {
    vec![
        ("PrincipalId", "principalId"),
        ("Role", "roleName"),
        ("Scope", "scope"),
    ]
}

/// `role definition list` — flattens each definition's `properties` up for table output.
pub async fn list_definitions(
    client: &ArmClient,
    scope: &str,
    name: Option<&str>,
) -> Result<Value> {
    let mut path = format!("{scope}/providers/Microsoft.Authorization/roleDefinitions");
    if let Some(n) = name {
        path.push_str(&format!("?{}", crate::odata::odata_eq("roleName", n)));
    }
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::map_list(&body, |d| {
        json!({
            "name": d.get("name").and_then(Value::as_str).unwrap_or(""),
            "type": d.get("type").and_then(Value::as_str).unwrap_or(""),
            "roleName": d.pointer("/properties/roleName").and_then(Value::as_str).unwrap_or(""),
            "id": d.get("id").and_then(Value::as_str).unwrap_or(""),
        })
    }))
}

/// Resolve a role given by name (e.g. "Contributor") or GUID into a full roleDefinition id.
async fn resolve_role_definition_id(
    client: &ArmClient,
    subscription: &str,
    role: &str,
) -> Result<String> {
    let is_guid = role.len() == 36 && role.split('-').count() == 5;
    if is_guid {
        return Ok(format!(
            "/subscriptions/{subscription}/providers/Microsoft.Authorization/roleDefinitions/{role}"
        ));
    }
    let path = format!(
        "/subscriptions/{subscription}/providers/Microsoft.Authorization/roleDefinitions?{}",
        crate::odata::odata_eq("roleName", role)
    );
    client
        .get(&path, API_VERSION)
        .await?
        .pointer("/value/0/id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| RazError::NotFound(format!("role '{role}'")))
}

/// `role assignment list` — assignments at `scope`, optionally filtered by principal object id.
pub async fn list_assignments(
    client: &ArmClient,
    scope: &str,
    assignee: Option<&str>,
) -> Result<Value> {
    let mut path = format!("{scope}/providers/Microsoft.Authorization/roleAssignments");
    if let Some(a) = assignee {
        path.push_str(&format!("?{}", crate::odata::odata_eq("principalId", a)));
    }
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::map_list(&body, flatten_assignment))
}

/// `role assignment create` — assign `role` (name or GUID) to `principal_id` at `scope`.
pub async fn create_assignment(
    client: &ArmClient,
    subscription: &str,
    scope: &str,
    role: &str,
    principal_id: &str,
    principal_type: Option<&str>,
) -> Result<Value> {
    let role_definition_id = resolve_role_definition_id(client, subscription, role).await?;
    let name = uuid::Uuid::new_v4().to_string();
    let path = format!("{scope}/providers/Microsoft.Authorization/roleAssignments/{name}");

    let mut props = json!({
        "roleDefinitionId": role_definition_id,
        "principalId": principal_id,
    });
    if let Some(pt) = principal_type {
        props["principalType"] = json!(pt);
    }
    let assignment = client
        .put(&path, API_VERSION, &json!({ "properties": props }))
        .await?;
    Ok(flatten_assignment(&assignment))
}

/// `role assignment delete` — remove the assignment of `role` to `principal_id` at `scope`.
pub async fn delete_assignment(
    client: &ArmClient,
    subscription: &str,
    scope: &str,
    role: &str,
    principal_id: &str,
) -> Result<()> {
    let role_definition_id = resolve_role_definition_id(client, subscription, role).await?;
    let path = format!(
        "{scope}/providers/Microsoft.Authorization/roleAssignments?{}",
        crate::odata::odata_eq("principalId", principal_id)
    );
    let body = client.get(&path, API_VERSION).await?;
    let id = body
        .get("value")
        .and_then(Value::as_array)
        .and_then(|v| {
            v.iter().find(|a| {
                a.pointer("/properties/roleDefinitionId")
                    .and_then(Value::as_str)
                    == Some(role_definition_id.as_str())
            })
        })
        .and_then(|a| a.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            RazError::NotFound(format!(
                "assignment of '{role}' to {principal_id} at this scope"
            ))
        })?
        .to_string();
    client.delete(&id, API_VERSION).await
}

/// Lift the useful `properties` fields up to the top level for table/JSON output.
fn flatten_assignment(a: &Value) -> Value {
    json!({
        "name": a.get("name").and_then(Value::as_str).unwrap_or(""),
        "principalId": a.pointer("/properties/principalId").and_then(Value::as_str).unwrap_or(""),
        "roleDefinitionId": a.pointer("/properties/roleDefinitionId").and_then(Value::as_str).unwrap_or(""),
        "roleName": a.pointer("/properties/roleDefinitionId").and_then(Value::as_str)
            .and_then(|id| id.rsplit('/').next()).unwrap_or(""),
        "scope": a.pointer("/properties/scope").and_then(Value::as_str).unwrap_or(""),
    })
}
