//! `raz wait` — poll a resource until a condition holds (az `... wait`). Generic over any ARM id.

use std::time::Duration;

use serde_json::Value;

use super::client::ArmClient;
use crate::error::{RazError, Result};

/// What to wait for.
pub enum WaitFor {
    /// `provisioningState` is `Succeeded`.
    Created,
    /// The resource no longer exists (404).
    Deleted,
    /// The resource exists (any successful GET).
    Exists,
    /// A JMESPath expression over the resource is truthy.
    Custom(String),
}

/// True for a JMESPath result that az treats as satisfied: non-null, non-false, non-empty.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}

/// Poll `id` every `interval` seconds (up to `timeout` seconds) until `condition` holds, returning
/// the final resource (or null for `Deleted`).
pub async fn wait(
    client: &ArmClient,
    id: &str,
    api_version: &str,
    condition: &WaitFor,
    interval: u64,
    timeout: u64,
) -> Result<Value> {
    let interval = interval.max(1);
    let attempts = (timeout / interval).max(1);
    for _ in 0..attempts {
        match client.get(id, api_version).await {
            Ok(resource) => match condition {
                WaitFor::Exists => return Ok(resource),
                WaitFor::Created => {
                    let state = resource
                        .pointer("/properties/provisioningState")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if state == "Succeeded" {
                        return Ok(resource);
                    }
                    if state == "Failed" || state == "Canceled" {
                        return Err(RazError::Http(format!("provisioning {state} for {id}")));
                    }
                }
                WaitFor::Custom(expr) => {
                    if truthy(&crate::output::apply_query(&resource, expr)) {
                        return Ok(resource);
                    }
                }
                WaitFor::Deleted => {} // still present; keep waiting
            },
            Err(RazError::NotFound(_)) => {
                if let WaitFor::Deleted = condition {
                    return Ok(Value::Null);
                }
                // otherwise not there yet; keep waiting
            }
            Err(e) => return Err(e),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
    Err(RazError::Http(format!("timed out waiting for {id}")))
}
