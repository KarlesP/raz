//! Azure Kubernetes Service (az `aks`) via ARM `Microsoft.ContainerService/managedClusters` —
//! list/show plus `get-credentials` (the cluster-user kubeconfig).

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2024-09-01";
const PROVIDER: &str = "Microsoft.ContainerService/managedClusters";

pub fn table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
        ("K8sVersion", "kubernetesVersion"),
    ]
}

fn cluster_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/{PROVIDER}/{name}"
    )
}

fn flatten(v: &Value) -> Value {
    let mut v = v.clone();
    super::enrich_resource(&mut v);
    json!({
        "name": v.get("name").and_then(Value::as_str).unwrap_or(""),
        "resourceGroup": v.get("resourceGroup").and_then(Value::as_str).unwrap_or(""),
        "location": v.get("location").and_then(Value::as_str).unwrap_or(""),
        "kubernetesVersion": v.pointer("/properties/kubernetesVersion").and_then(Value::as_str).unwrap_or(""),
    })
}

/// `raz aks list`.
pub async fn list(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/{PROVIDER}");
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::map_list(&body, flatten))
}

/// `raz aks show`.
pub async fn show(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<Value> {
    let body = client
        .get(
            &cluster_path(subscription, resource_group, name),
            API_VERSION,
        )
        .await?;
    Ok(flatten(&body))
}

/// `raz aks get-credentials` — POST `listClusterUserCredential`; returns the first kubeconfig's
/// base64-encoded `value` (decoded by the command layer).
pub async fn get_credentials(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<String> {
    let path = format!(
        "{}/listClusterUserCredential",
        cluster_path(subscription, resource_group, name)
    );
    let body = client.post(&path, API_VERSION, None).await?;
    body.pointer("/kubeconfigs/0/value")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| crate::error::RazError::NotFound("kubeconfig for cluster".into()))
}
