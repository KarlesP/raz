//! Azure Monitor (az `monitor`) — metric values and the activity log, via Microsoft.Insights.

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::Result;
use crate::output::TableSpec;

const METRICS_API: &str = "2018-01-01";
const ACTIVITY_API: &str = "2015-04-01";

pub fn metrics_table_spec() -> TableSpec {
    vec![("Metric", "metric"), ("Unit", "unit"), ("Latest", "latest")]
}

pub fn activity_table_spec() -> TableSpec {
    vec![
        ("Time", "time"),
        ("Operation", "operation"),
        ("Status", "status"),
        ("ResourceGroup", "resourceGroup"),
    ]
}

/// `monitor metrics list` — latest value per metric for a resource. `metricnames` is a
/// comma-separated list; `aggregation` is Average/Total/Minimum/Maximum/Count.
pub async fn metrics(
    client: &ArmClient,
    resource_id: &str,
    metricnames: &str,
    aggregation: &str,
    interval: Option<&str>,
) -> Result<Value> {
    let names = metricnames.replace(' ', "%20");
    let mut query = format!("metricnames={names}&aggregation={aggregation}");
    if let Some(i) = interval {
        query.push_str(&format!("&interval={i}"));
    }
    let path = format!("{resource_id}/providers/microsoft.insights/metrics?{query}");
    let body = client.get(&path, METRICS_API).await?;
    let agg = aggregation.to_lowercase();
    Ok(super::map_list(&body, |m| {
        let latest = m
            .pointer("/timeseries/0/data")
            .and_then(Value::as_array)
            .and_then(|d| d.last())
            .and_then(|p| p.get(agg.as_str()))
            .cloned()
            .unwrap_or(Value::Null);
        json!({
            "metric": m.pointer("/name/value").and_then(Value::as_str).unwrap_or(""),
            "unit": m.get("unit").and_then(Value::as_str).unwrap_or(""),
            "latest": latest,
        })
    }))
}

/// `monitor activity-log list` — management events between `start` and `end` (RFC3339), capped at
/// `max` entries from the first page.
pub async fn activity_log(
    client: &ArmClient,
    subscription: &str,
    start: &str,
    end: &str,
    max: usize,
) -> Result<Value> {
    let filter = format!("eventTimestamp ge '{start}' and eventTimestamp le '{end}'")
        .replace(' ', "%20")
        .replace('\'', "%27");
    let path = format!(
        "/subscriptions/{subscription}/providers/microsoft.insights/eventtypes/management/values?$filter={filter}"
    );
    let body = client.get(&path, ACTIVITY_API).await?;
    let rows = body
        .get("value")
        .and_then(Value::as_array)
        .map(|events| {
            events
                .iter()
                .take(max)
                .map(|e| {
                    json!({
                        "time": e.get("eventTimestamp").and_then(Value::as_str).unwrap_or(""),
                        "operation": e.pointer("/operationName/localizedValue").and_then(Value::as_str).unwrap_or(""),
                        "status": e.pointer("/status/value").and_then(Value::as_str).unwrap_or(""),
                        "resourceGroup": e.get("resourceGroupName").and_then(Value::as_str).unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Value::Array(rows))
}
