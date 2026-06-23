//! Virtual machine operations (az `vm`). `create` orchestrates the resources a VM needs
//! (resource group, virtual network + subnet, NIC) then the VM, defaulting to West Europe
//! and a small Ubuntu image. `update` patches size/tags; `delete` removes the VM.
//! `start`/`stop` are not yet implemented (they are action-style long-running operations).

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::{usage, RazError, Result};
use crate::output::TableSpec;

const API_VERSION: &str = "2024-07-01";
const NETWORK_API: &str = "2023-09-01";
const PROVIDER: &str = "Microsoft.Compute/virtualMachines";

/// Default small burstable size and Ubuntu 22.04 LTS (gen2) image for `raz vm create`.
pub const DEFAULT_VM_SIZE: &str = "Standard_B1s";

fn vm_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/{PROVIDER}/{name}"
    )
}

/// Inputs for [`create`]. Network resources default to `<vm>-vnet` / `default` subnet and are
/// created if absent. Exactly one of `ssh_key` / `admin_password` must be provided.
pub struct VmCreate<'a> {
    pub subscription: &'a str,
    pub resource_group: &'a str,
    pub name: &'a str,
    pub location: &'a str,
    pub size: &'a str,
    pub admin_username: &'a str,
    pub ssh_key: Option<&'a str>,
    pub admin_password: Option<&'a str>,
}

/// Columns shown in `--output table`, matching az's vm table shape.
pub fn table_spec() -> TableSpec {
    TableSpec::new(vec![
        ("Name", "name"),
        ("ResourceGroup", "resourceGroup"),
        ("Location", "location"),
    ])
}

/// `raz vm list` — all virtual machines in the subscription.
pub async fn list(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/{PROVIDER}");
    let body = client.get(&path, API_VERSION).await?;
    Ok(normalize_list(body))
}

/// `raz vm show -g <rg> -n <name>` — a single virtual machine.
pub async fn show(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<Value> {
    let path = vm_path(subscription, resource_group, name);
    let mut body = client.get(&path, API_VERSION).await?;
    super::enrich_resource(&mut body);
    Ok(body)
}

/// `raz vm create` — orchestrate resource group + virtual network/subnet + NIC + the VM, then
/// wait for the VM to finish provisioning. Linux only; defaults to West Europe.
pub async fn create(client: &ArmClient, args: &VmCreate<'_>) -> Result<Value> {
    if args.ssh_key.is_none() && args.admin_password.is_none() {
        return Err(usage(
            "provide --ssh-key-value or --admin-password to authenticate the VM",
        ));
    }

    let VmCreate {
        subscription,
        resource_group,
        name,
        location,
        size,
        admin_username,
        ..
    } = *args;

    // 0. Make sure the providers this command touches are registered (az does this silently).
    client
        .ensure_provider_registered(subscription, "Microsoft.Network")
        .await?;
    client
        .ensure_provider_registered(subscription, "Microsoft.Compute")
        .await?;

    // 1. Resource group.
    client
        .ensure_resource_group(subscription, resource_group, location)
        .await?;

    // 2. Virtual network + subnet (create the convenience network if it does not exist yet).
    let vnet_name = format!("{name}-vnet");
    let subnet_name = "default";
    let subnet_path = format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Network/virtualNetworks/{vnet_name}/subnets/{subnet_name}"
    );
    let subnet_id = match client.get(&subnet_path, NETWORK_API).await {
        Ok(subnet) => subnet
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Err(RazError::NotFound(_)) => {
            super::vnet::create(
                client,
                &super::vnet::VnetCreate {
                    subscription,
                    resource_group,
                    name: &vnet_name,
                    location,
                    address_prefix: "10.0.0.0/16",
                    subnet_name,
                    subnet_prefix: "10.0.0.0/24",
                },
            )
            .await?;
            // Subnet id is deterministic once the vnet exists.
            format!(
                "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Network/virtualNetworks/{vnet_name}/subnets/{subnet_name}"
            )
        }
        Err(e) => return Err(e),
    };

    // 3. NIC.
    let nic_name = format!("{name}-nic");
    let nic_path = format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Network/networkInterfaces/{nic_name}"
    );
    let nic_body = json!({
        "location": location,
        "properties": {
            "ipConfigurations": [{
                "name": "ipconfig1",
                "properties": {
                    "subnet": { "id": subnet_id },
                    "privateIPAllocationMethod": "Dynamic"
                }
            }]
        }
    });
    client.put(&nic_path, NETWORK_API, &nic_body).await?;
    let nic = client.wait_provisioning(&nic_path, NETWORK_API).await?;
    let nic_id = nic
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(&nic_path)
        .to_string();

    // 4. The VM.
    let mut os_profile = json!({
        "computerName": name,
        "adminUsername": admin_username,
    });
    match (args.ssh_key, args.admin_password) {
        (Some(key), _) => {
            os_profile["linuxConfiguration"] = json!({
                "disablePasswordAuthentication": true,
                "ssh": { "publicKeys": [{
                    "path": format!("/home/{admin_username}/.ssh/authorized_keys"),
                    "keyData": key
                }]}
            });
        }
        (None, Some(pw)) => {
            os_profile["adminPassword"] = Value::String(pw.to_string());
            os_profile["linuxConfiguration"] = json!({ "disablePasswordAuthentication": false });
        }
        (None, None) => unreachable!("validated above"),
    }

    let vm_path = vm_path(subscription, resource_group, name);
    let vm_body = json!({
        "location": location,
        "properties": {
            "hardwareProfile": { "vmSize": size },
            "storageProfile": {
                "imageReference": {
                    "publisher": "Canonical",
                    "offer": "0001-com-ubuntu-server-jammy",
                    "sku": "22_04-lts-gen2",
                    "version": "latest"
                },
                "osDisk": {
                    "createOption": "FromImage",
                    "managedDisk": { "storageAccountType": "Standard_LRS" }
                }
            },
            "osProfile": os_profile,
            "networkProfile": {
                "networkInterfaces": [{ "id": nic_id }]
            }
        }
    });
    client.put(&vm_path, API_VERSION, &vm_body).await?;
    let mut final_state = client.wait_provisioning(&vm_path, API_VERSION).await?;
    super::enrich_resource(&mut final_state);
    Ok(final_state)
}

/// `raz vm update` — patch an existing VM's size and/or tags (read-modify-write), then wait.
pub async fn update(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    size: Option<&str>,
    tags: &[(String, String)],
) -> Result<Value> {
    let path = vm_path(subscription, resource_group, name);
    let mut resource = client.get(&path, API_VERSION).await?;

    if let Some(size) = size {
        if let Some(hw) = resource.pointer_mut("/properties/hardwareProfile") {
            hw["vmSize"] = Value::String(size.to_string());
        }
    }
    if !tags.is_empty() {
        let map = resource
            .as_object_mut()
            .expect("vm resource is an object")
            .entry("tags")
            .or_insert_with(|| json!({}));
        if let Some(obj) = map.as_object_mut() {
            for (k, v) in tags {
                obj.insert(k.clone(), Value::String(v.clone()));
            }
        }
    }

    if let Some(props) = resource
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        props.remove("provisioningState");
    }

    client.put(&path, API_VERSION, &resource).await?;
    let mut final_state = client.wait_provisioning(&path, API_VERSION).await?;
    super::enrich_resource(&mut final_state);
    Ok(final_state)
}

/// `raz vm delete` — delete a VM and wait for the operation to finish.
pub async fn delete(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
) -> Result<()> {
    let path = vm_path(subscription, resource_group, name);
    client.delete(&path, API_VERSION).await?;
    client.wait_deleted(&path, API_VERSION).await
}

/// `raz vm start` — not yet implemented (POST `/start`, then poll the operation).
pub async fn start(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented("vm start".into()))
}

/// `raz vm stop` — not yet implemented (POST `/powerOff`, then poll the operation).
pub async fn stop(_name: &str) -> Result<Value> {
    Err(RazError::NotImplemented("vm stop".into()))
}

fn normalize_list(body: Value) -> Value {
    let mut items = body
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in &mut items {
        super::enrich_resource(item);
    }
    Value::Array(items)
}
