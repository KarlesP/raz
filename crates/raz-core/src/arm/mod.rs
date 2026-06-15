//! Azure Resource Manager (ARM) REST access and the vnet/vm operations.
//!
//! There are no management-plane SDK crates in the vendored azure-sdk-for-rust, so we call
//! the ARM REST API directly. [`client::ArmClient`] is the thin transport (the production
//! analogue would build an `azure_core::http::Pipeline` with a `BearerTokenPolicy`); the
//! [`vnet`] and [`vm`] modules hold the resource operations, mirroring az's per-service
//! command modules.

pub mod client;
pub mod vm;
pub mod vnet;

use serde_json::Value;

/// Add a derived `resourceGroup` field to an ARM resource object by parsing its `id`
/// (`/subscriptions/{s}/resourceGroups/{rg}/...`). az exposes this same convenience field;
/// it makes table output useful and is harmless in JSON output.
pub(crate) fn enrich_resource(item: &mut Value) {
    let rg = item
        .get("id")
        .and_then(Value::as_str)
        .and_then(resource_group_from_id);
    if let (Some(rg), Some(obj)) = (rg, item.as_object_mut()) {
        obj.entry("resourceGroup").or_insert(Value::String(rg));
    }
}

/// Extract the resource group (case-insensitive segment match) from an ARM resource id.
fn resource_group_from_id(id: &str) -> Option<String> {
    let parts: Vec<&str> = id.split('/').collect();
    parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("resourceGroups"))
        .and_then(|i| parts.get(i + 1))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_resource_group_from_id() {
        let id = "/subscriptions/s1/resourceGroups/my-rg/providers/Microsoft.Compute/virtualMachines/vm1";
        assert_eq!(resource_group_from_id(id).as_deref(), Some("my-rg"));
        assert_eq!(resource_group_from_id("/subscriptions/s1"), None);
    }

    #[test]
    fn enrich_adds_resource_group_field() {
        let mut v = json!({
            "id": "/subscriptions/s/resourceGroups/rg7/providers/Microsoft.Network/virtualNetworks/vnet1",
            "name": "vnet1"
        });
        enrich_resource(&mut v);
        assert_eq!(v["resourceGroup"], "rg7");
    }
}
