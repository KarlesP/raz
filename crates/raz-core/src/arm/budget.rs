//! Cost-management budgets via `Microsoft.Consumption/budgets` (az `consumption budget`). Scope is
//! a subscription or a resource group (see [`super::role::scope`]).

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const API_VERSION: &str = "2023-11-01";

pub fn table_spec() -> TableSpec {
    vec![
        ("Name", "name"),
        ("Amount", "amount"),
        ("TimeGrain", "timeGrain"),
    ]
}

fn budget_path(scope: &str, name: &str) -> String {
    format!("{scope}/providers/Microsoft.Consumption/budgets/{name}")
}

/// Build the budget request body. `start`/`end` are ISO timestamps (e.g. `2026-07-01T00:00:00Z`).
fn build_body(amount: f64, time_grain: &str, start: &str, end: Option<&str>) -> Value {
    let mut period = json!({ "startDate": start });
    if let Some(e) = end {
        period["endDate"] = json!(e);
    }
    json!({
        "properties": {
            "category": "Cost",
            "amount": amount,
            "timeGrain": time_grain,
            "timePeriod": period,
        }
    })
}

fn flatten(b: &Value) -> Value {
    json!({
        "name": b.get("name").and_then(Value::as_str).unwrap_or(""),
        "amount": b.pointer("/properties/amount").cloned().unwrap_or(Value::Null),
        "timeGrain": b.pointer("/properties/timeGrain").and_then(Value::as_str).unwrap_or(""),
    })
}

/// `raz budget create`.
pub async fn create(
    client: &ArmClient,
    scope: &str,
    name: &str,
    amount: f64,
    time_grain: &str,
    start: &str,
    end: Option<&str>,
) -> Result<Value> {
    let body = build_body(amount, time_grain, start, end);
    let resp = client
        .put(&budget_path(scope, name), API_VERSION, &body)
        .await?;
    Ok(flatten(&resp))
}

/// `raz budget update` — change amount / time-grain / end-date on an existing budget
/// (read-modify-write; unset fields keep their current value).
pub async fn update(
    client: &ArmClient,
    scope: &str,
    name: &str,
    amount: Option<f64>,
    time_grain: Option<&str>,
    end: Option<&str>,
) -> Result<Value> {
    let path = budget_path(scope, name);
    let existing = client.get(&path, API_VERSION).await?;
    let mut props = existing
        .get("properties")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(a) = amount {
        props["amount"] = json!(a);
    }
    if let Some(tg) = time_grain {
        props["timeGrain"] = json!(tg);
    }
    if let Some(e) = end {
        props["timePeriod"]["endDate"] = json!(e);
    }
    let resp = client
        .put(&path, API_VERSION, &json!({ "properties": props }))
        .await?;
    Ok(flatten(&resp))
}

/// `raz budget list`.
pub async fn list(client: &ArmClient, scope: &str) -> Result<Value> {
    let path = format!("{scope}/providers/Microsoft.Consumption/budgets");
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::map_list(&body, flatten))
}

/// `raz budget delete`.
pub async fn delete(client: &ArmClient, scope: &str, name: &str) -> Result<()> {
    client.delete(&budget_path(scope, name), API_VERSION).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_includes_optional_end() {
        let with = build_body(
            100.0,
            "Monthly",
            "2026-07-01T00:00:00Z",
            Some("2027-07-01T00:00:00Z"),
        );
        assert_eq!(
            with["properties"]["timePeriod"]["endDate"],
            "2027-07-01T00:00:00Z"
        );
        let without = build_body(100.0, "Monthly", "2026-07-01T00:00:00Z", None);
        assert!(without["properties"]["timePeriod"].get("endDate").is_none());
        assert_eq!(without["properties"]["amount"], 100.0);
    }
}
