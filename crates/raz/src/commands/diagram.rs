//! `raz diagram` — emit a Mermaid topology of the subscription (or a resource group). The TUI
//! reuses the same `raz_core::diagram` model to render ASCII; the CLI is Mermaid-only.

use clap::Args;
use serde_json::Value;

use raz_core::arm::{network, resource, vnet};
use raz_core::diagram;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::arm_context;

#[derive(Args)]
pub struct DiagramArgs {
    /// Limit to one resource group.
    #[arg(long, short = 'g')]
    resource_group: Option<String>,
    /// Skip network detail fetches — inventory only (no vnet→subnet→nic→vm edges).
    #[arg(long)]
    no_network: bool,
}

pub async fn run(args: DiagramArgs, globals: GlobalArgs) -> Result<()> {
    let (_ctx, client, sub) = arm_context(globals).await?;
    let rg = args.resource_group.as_deref();

    let resources = resource::list(&client, &sub, rg, None).await?;

    let (mut vnet_details, mut nic_details) = (Vec::new(), Vec::new());
    if !args.no_network {
        for (g, name) in list_targets(&vnet::list(&client, &sub).await?, rg) {
            if let Ok(v) = vnet::show(&client, &sub, &g, &name).await {
                vnet_details.push(v);
            }
        }
        let nics = network::list(&client, &sub, "networkInterfaces").await?;
        for (g, name) in list_targets(&nics, rg) {
            if let Ok(n) = network::show(&client, &sub, &g, "networkInterfaces", &name).await {
                nic_details.push(n);
            }
        }
    }

    let topology = diagram::build(&sub, &resources, &vnet_details, &nic_details);
    print!("{}", diagram::to_mermaid(&topology));
    Ok(())
}

/// Extract `(resource_group, name)` pairs from a listing, optionally filtered to one group.
fn list_targets(list: &Value, rg: Option<&str>) -> Vec<(String, String)> {
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let g = r.get("resourceGroup").and_then(Value::as_str)?;
                    let name = r.get("name").and_then(Value::as_str)?;
                    if rg.is_some_and(|want| !want.eq_ignore_ascii_case(g)) {
                        return None;
                    }
                    Some((g.to_string(), name.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}
