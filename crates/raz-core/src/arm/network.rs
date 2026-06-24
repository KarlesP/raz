//! Network resource breadth (az `network nsg|public-ip|nic`) — list/show across the common
//! Microsoft.Network types. Each type is the same ARM shape, parameterized by its path segment.

use serde_json::Value;

use super::client::ArmClient;
use crate::error::Result;

const API_VERSION: &str = "2023-09-01";

/// `network <type> list` — every resource of `segment` (e.g. `networkSecurityGroups`) in the sub.
pub async fn list(client: &ArmClient, subscription: &str, segment: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/Microsoft.Network/{segment}");
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::enrich_list(body))
}

/// `network <type> show -g -n`.
pub async fn show(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    segment: &str,
    name: &str,
) -> Result<Value> {
    let path = format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Network/{segment}/{name}"
    );
    let mut body = client.get(&path, API_VERSION).await?;
    super::enrich_resource(&mut body);
    Ok(body)
}
