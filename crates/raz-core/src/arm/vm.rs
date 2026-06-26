//! Virtual machine operations (az `vm`). `create` orchestrates the resources a VM needs
//! (resource group, virtual network + subnet, NIC) then the VM, defaulting to West Europe
//! and a small Ubuntu image. `update` patches size/tags; `delete` removes the VM.
//! `start`/`stop`/`restart`/`deallocate` POST the VM power actions and poll the operation.

use serde_json::{json, Value};

use super::client::ArmClient;
use crate::error::{usage, RazError, Result};

const API_VERSION: &str = "2024-07-01";
const NETWORK_API: &str = "2023-09-01";
const SKUS_API: &str = "2021-07-01";
const PROVIDER: &str = "Microsoft.Compute/virtualMachines";

/// Default small burstable size and Ubuntu 22.04 LTS (gen2) image for `raz vm create`.
pub const DEFAULT_VM_SIZE: &str = "Standard_B1s";

fn vm_path(subscription: &str, resource_group: &str, name: &str) -> String {
    format!(
        "/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/{PROVIDER}/{name}"
    )
}

/// Compute resource-SKUs query filtered to one region (used by the create pre-flight and
/// `list-sizes`).
fn skus_filter_path(subscription: &str, location: &str) -> String {
    format!(
        "/subscriptions/{subscription}/providers/Microsoft.Compute/skus?{}",
        crate::odata::odata_eq("location", location)
    )
}

/// Inputs for [`create`], mirroring `az vm create`'s networking: the VNet/subnet are reused if
/// they already exist and created otherwise; a public IP and NSG are created and attached unless
/// opted out (`public_ip`/`nsg` = `None`). Exactly one of `ssh_key` / `admin_password` is required.
pub struct VmCreate<'a> {
    pub subscription: &'a str,
    pub resource_group: &'a str,
    pub name: &'a str,
    pub location: &'a str,
    pub size: &'a str,
    pub admin_username: &'a str,
    pub ssh_key: Option<&'a str>,
    pub admin_password: Option<&'a str>,
    /// VNet to use or create (defaults to `<vm>-vnet` at the command layer).
    pub vnet_name: &'a str,
    /// Subnet within `vnet_name` to use or create (defaults to `default`).
    pub subnet_name: &'a str,
    /// Public IP name to create and attach, or `None` to skip (az `--public-ip-address ""`).
    pub public_ip: Option<&'a str>,
    /// NSG name to create and attach to the NIC, or `None` to skip (az `--nsg ""`).
    pub nsg: Option<&'a str>,
}

/// az-style optional resource: no flag → create with `default` name; explicit `""` → skip; else
/// use the given name. Used to resolve `--public-ip-address` / `--nsg`.
pub fn optional_resource_name(flag: Option<&str>, default: &str) -> Option<String> {
    match flag {
        Some("") => None,
        Some(name) => Some(name.to_string()),
        None => Some(default.to_string()),
    }
}

/// `raz vm list` — all virtual machines in the subscription.
pub async fn list(client: &ArmClient, subscription: &str) -> Result<Value> {
    let path = format!("/subscriptions/{subscription}/providers/{PROVIDER}");
    let body = client.get(&path, API_VERSION).await?;
    Ok(super::enrich_list(body))
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

    // 0b. Pre-flight: reject an unavailable size now, before creating any resources, so the
    // user gets a clear up-front error instead of a half-built deployment.
    ensure_size_available(client, subscription, location, size).await?;

    // 1. Resource group.
    client
        .ensure_resource_group(subscription, resource_group, location)
        .await?;

    // 2. Virtual network + subnet: reuse if present (lets the VM join an existing/spoke subnet),
    // else create the convenience network — matching `az vm create --vnet-name/--subnet`.
    let vnet_name = args.vnet_name;
    let subnet_name = args.subnet_name;
    let net = |kind: &str, n: &str| {
        format!("/subscriptions/{subscription}/resourceGroups/{resource_group}/providers/Microsoft.Network/{kind}/{n}")
    };
    let subnet_path = format!(
        "{}/subnets/{subnet_name}",
        net("virtualNetworks", vnet_name)
    );
    let subnet_id = match client.get(&subnet_path, NETWORK_API).await {
        Ok(subnet) => subnet
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(&subnet_path)
            .to_string(),
        Err(RazError::NotFound(_)) => {
            super::vnet::create(
                client,
                &super::vnet::VnetCreate {
                    subscription,
                    resource_group,
                    name: vnet_name,
                    location,
                    address_prefix: "10.0.0.0/16",
                    subnet_name,
                    subnet_prefix: "10.0.0.0/24",
                },
            )
            .await?;
            subnet_path.clone() // deterministic once the vnet exists
        }
        Err(e) => return Err(e),
    };

    // 2b. Optional NSG (az creates one by default; `--nsg ""` → None skips it).
    let nsg_id = match args.nsg {
        Some(nsg_name) => {
            let path = net("networkSecurityGroups", nsg_name);
            let body = json!({ "location": location, "properties": {} });
            client.put(&path, NETWORK_API, &body).await?;
            let r = client.wait_provisioning(&path, NETWORK_API).await?;
            Some(
                r.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or(&path)
                    .to_string(),
            )
        }
        None => None,
    };

    // 2c. Optional public IP (az creates one by default; `--public-ip-address ""` → None skips it).
    let pip_id = match args.public_ip {
        Some(pip_name) => {
            let path = net("publicIPAddresses", pip_name);
            let body = json!({
                "location": location,
                "sku": { "name": "Standard" },
                "properties": { "publicIPAllocationMethod": "Static" }
            });
            client.put(&path, NETWORK_API, &body).await?;
            let r = client.wait_provisioning(&path, NETWORK_API).await?;
            Some(
                r.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or(&path)
                    .to_string(),
            )
        }
        None => None,
    };

    // 3. NIC — subnet, plus the public IP / NSG when requested.
    let nic_name = format!("{name}-nic");
    let nic_path = net("networkInterfaces", &nic_name);
    let mut ipconfig = json!({
        "subnet": { "id": subnet_id },
        "privateIPAllocationMethod": "Dynamic"
    });
    if let Some(pid) = &pip_id {
        ipconfig["publicIPAddress"] = json!({ "id": pid });
    }
    let mut nic_props = json!({
        "ipConfigurations": [{ "name": "ipconfig1", "properties": ipconfig }]
    });
    if let Some(nid) = &nsg_id {
        nic_props["networkSecurityGroup"] = json!({ "id": nid });
    }
    let nic_body = json!({ "location": location, "properties": nic_props });
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

/// Pre-flight check that `size` can actually be deployed in `location` for this subscription,
/// using the Compute resource-SKUs API. Returns a clear error when the size is not offered or
/// is restricted (the `SkuNotAvailable` case the user hits at create time). Best-effort: if the
/// SKUs query itself fails, we proceed rather than introducing a new failure mode.
async fn ensure_size_available(
    client: &ArmClient,
    subscription: &str,
    location: &str,
    size: &str,
) -> Result<()> {
    let body = match client
        .get(&skus_filter_path(subscription, location), SKUS_API)
        .await
    {
        Ok(b) => b,
        Err(_) => return Ok(()),
    };
    let items = body.get("value").and_then(Value::as_array);
    let Some(items) = items else { return Ok(()) };

    for item in items {
        if item.get("resourceType").and_then(Value::as_str) != Some("virtualMachines")
            || item.get("name").and_then(Value::as_str) != Some(size)
        {
            continue;
        }
        // Found the size in this region; a Location-type restriction means it can't be deployed.
        let restricted = item
            .get("restrictions")
            .and_then(Value::as_array)
            .map(|rs| {
                rs.iter()
                    .any(|r| r.get("type").and_then(Value::as_str) == Some("Location"))
            })
            .unwrap_or(false);
        return if restricted {
            Err(usage(format!(
                "VM size '{size}' is currently not available in '{location}' (capacity/subscription restriction). Choose another --size or -l location."
            )))
        } else {
            Ok(())
        };
    }

    Err(usage(format!(
        "VM size '{size}' is not offered in location '{location}'. Choose another --size or -l location."
    )))
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

/// Power actions: POST the VM action endpoint and wait for the long-running op to finish.
async fn power_action(
    client: &ArmClient,
    subscription: &str,
    resource_group: &str,
    name: &str,
    action: &str,
) -> Result<()> {
    let path = format!("{}/{action}", vm_path(subscription, resource_group, name));
    client.post_action(&path, API_VERSION).await
}

/// `raz vm start`.
pub async fn start(client: &ArmClient, subscription: &str, rg: &str, name: &str) -> Result<()> {
    power_action(client, subscription, rg, name, "start").await
}

/// `raz vm stop` — power off (still billed for the allocation; use `deallocate` to stop billing).
pub async fn stop(client: &ArmClient, subscription: &str, rg: &str, name: &str) -> Result<()> {
    power_action(client, subscription, rg, name, "powerOff").await
}

/// `raz vm restart`.
pub async fn restart(client: &ArmClient, subscription: &str, rg: &str, name: &str) -> Result<()> {
    power_action(client, subscription, rg, name, "restart").await
}

/// `raz vm deallocate` — stop and release the compute (stops compute billing).
pub async fn deallocate(
    client: &ArmClient,
    subscription: &str,
    rg: &str,
    name: &str,
) -> Result<()> {
    power_action(client, subscription, rg, name, "deallocate").await
}

/// `raz vm list-sizes` — the VM sizes offered in `location`, from the Compute resource-SKUs API
/// (the same source the create pre-flight uses). Projects name + vCPUs + memory.
pub async fn list_sizes(client: &ArmClient, subscription: &str, location: &str) -> Result<Value> {
    let body = client
        .get(&skus_filter_path(subscription, location), SKUS_API)
        .await?;
    let items = body
        .get("value")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut sizes = Vec::new();
    for item in &items {
        if item.get("resourceType").and_then(Value::as_str) != Some("virtualMachines") {
            continue;
        }
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let cap = |key: &str| -> String {
            item.get("capabilities")
                .and_then(Value::as_array)
                .and_then(|caps| {
                    caps.iter()
                        .find(|c| c.get("name").and_then(Value::as_str) == Some(key))
                })
                .and_then(|c| c.get("value").and_then(Value::as_str))
                .unwrap_or("")
                .to_string()
        };
        sizes.push(json!({
            "name": name,
            "vCPUs": cap("vCPUs"),
            "memoryGB": cap("MemoryGB"),
            "location": location,
        }));
    }
    Ok(Value::Array(sizes))
}

#[cfg(test)]
mod tests {
    use super::optional_resource_name;

    #[test]
    fn optional_resource_name_az_semantics() {
        assert_eq!(
            optional_resource_name(None, "vm-pip"),
            Some("vm-pip".into())
        );
        assert_eq!(optional_resource_name(Some(""), "vm-pip"), None); // az --public-ip-address ""
        assert_eq!(
            optional_resource_name(Some("mypip"), "vm-pip"),
            Some("mypip".into())
        );
    }
}
