//! Governance scan: Azure Policy **assignments** (Microsoft.Authorization) joined with
//! **compliance** from Microsoft.PolicyInsights. Mirrors a mix of `az policy assignment list`
//! and `az policy state summarize`.
//!
//! Results are tiered by scope — subscription-wide assignments first, then resource-group
//! scoped — and can be filtered to a service (e.g. vnet/vm/storage).

use std::collections::HashMap;

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const ASSIGNMENTS_API: &str = "2022-06-01";
const INSIGHTS_API: &str = "2019-10-01";

pub fn table_spec() -> TableSpec {
    vec![
        ("Tier", "tier"),
        ("Assignment", "name"),
        ("Enforcement", "enforcement"),
        ("NonCompliant", "nonCompliant"),
        ("Definition", "definition"),
    ]
}

/// Scan policy governance for the subscription: list assignments, join per-assignment compliance
/// (best-effort), tier by scope (subscription before resource group), and optionally filter to a
/// service. `service = None` (or `--all`) returns everything.
pub async fn scan(client: &ArmClient, subscription: &str, service: Option<&str>) -> Result<Value> {
    let assignments = list_assignments(client, subscription).await?;
    let compliance = compliance_summary(client, subscription).await;
    let keywords = service.map(service_keywords);

    let mut rows: Vec<Value> = assignments
        .iter()
        .filter_map(|a| {
            let id = a.get("id").and_then(Value::as_str).unwrap_or("");
            let scope = a
                .pointer("/properties/scope")
                .and_then(Value::as_str)
                .unwrap_or("");
            let display = a
                .pointer("/properties/displayName")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| a.get("name").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
            let definition = a
                .pointer("/properties/policyDefinitionId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let enforcement = a
                .pointer("/properties/enforcementMode")
                .and_then(Value::as_str)
                .unwrap_or("Default");

            if let Some(kw) = &keywords {
                let hay = format!("{display} {definition}").to_lowercase();
                if !kw.iter().any(|k| hay.contains(k.as_str())) {
                    return None;
                }
            }

            let tier = if scope.to_lowercase().contains("/resourcegroups/") {
                "ResourceGroup"
            } else {
                "Subscription"
            };
            let non_compliant = match compliance.get(&id.to_lowercase()) {
                Some(n) => json!(n),
                None => json!("n/a"),
            };
            Some(json!({
                "tier": tier,
                "name": display,
                "scope": scope,
                "enforcement": enforcement,
                "definition": definition.rsplit('/').next().unwrap_or(definition),
                "nonCompliant": non_compliant,
            }))
        })
        .collect();

    // Subscription-wide assignments first, then resource-group scoped.
    rows.sort_by_key(|r| if r["tier"] == "Subscription" { 0 } else { 1 });
    Ok(Value::Array(rows))
}

/// All policy assignments that apply at or below the subscription.
async fn list_assignments(client: &ArmClient, subscription: &str) -> Result<Vec<Value>> {
    let path = format!(
        "/subscriptions/{subscription}/providers/Microsoft.Authorization/policyAssignments"
    );
    let body = client.get(&path, ASSIGNMENTS_API).await?;
    Ok(body
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// Per-assignment non-compliant-resource counts from PolicyInsights `summarize`. Best-effort:
/// returns an empty map if the call fails (so the scan still shows assignments).
async fn compliance_summary(client: &ArmClient, subscription: &str) -> HashMap<String, i64> {
    let path = format!(
        "/subscriptions/{subscription}/providers/Microsoft.PolicyInsights/policyStates/latest/summarize"
    );
    let Ok(body) = client.post(&path, INSIGHTS_API, None).await else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    if let Some(assignments) = body
        .pointer("/value/0/policyAssignments")
        .and_then(Value::as_array)
    {
        for a in assignments {
            let id = a
                .get("policyAssignmentId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            let non_compliant = a
                .pointer("/results/nonCompliantResources")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if !id.is_empty() {
                map.insert(id, non_compliant);
            }
        }
    }
    map
}

/// Map a service keyword to substrings to match against an assignment's name + definition id.
fn service_keywords(service: &str) -> Vec<String> {
    let s = service.to_lowercase();
    let mapped: &[&str] = match s.as_str() {
        "vnet" | "network" | "virtual-network" => &["vnet", "virtual network", "microsoft.network"],
        "vm" | "compute" | "virtual-machine" => &["virtual machine", "microsoft.compute"],
        "storage" | "st" | "storage-account" => &["storage", "microsoft.storage"],
        "keyvault" | "kv" | "key-vault" => &["key vault", "keyvault", "microsoft.keyvault"],
        "sql" => &["sql", "microsoft.sql"],
        "aks" | "kubernetes" => &["kubernetes", "aks", "microsoft.containerservice"],
        "tag" | "tags" | "tagging" => &["tag"],
        _ => &[],
    };
    if mapped.is_empty() {
        vec![s]
    } else {
        mapped.iter().map(|k| k.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_keywords_map_and_fallback() {
        assert!(service_keywords("vnet").contains(&"microsoft.network".to_string()));
        assert_eq!(service_keywords("widget"), vec!["widget".to_string()]);
    }
}
