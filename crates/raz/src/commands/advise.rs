//! `raz advise` — analyze the active subscription's architecture (CAF naming, ALZ tags, governance)
//! and surface a profile + guardrail warnings, so you don't ship out of scope. Phase 1 is
//! model-free; `--summary` (a GGUF/endpoint advisor) is planned.

use clap::Args;

use raz_core::arm::{group, policy, resource};
use raz_core::error::Result;
use raz_core::{advisor, GlobalArgs};

use super::arm_context;

#[derive(Args)]
pub struct AdviseArgs {
    /// Limit the scan to a resource type (e.g. Microsoft.Compute/virtualMachines).
    #[arg(long)]
    service: Option<String>,
    /// Emit the structured profile as JSON instead of the readable report.
    #[arg(long)]
    json: bool,
}

pub async fn run(args: AdviseArgs, globals: GlobalArgs) -> Result<()> {
    let (_ctx, client, sub) = arm_context(globals).await?;
    let resources = resource::list(&client, &sub, None, args.service.as_deref()).await?;
    let groups = group::list(&client, &sub).await?;
    let governance = policy::scan(&client, &sub, None).await?;

    let profile = advisor::analyze(&resources, &groups, &governance, &sub);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        print_report(&profile);
    }
    Ok(())
}

fn print_report(p: &advisor::Profile) {
    println!("Architecture profile — subscription {}", p.subscription);
    println!(
        "  Resources: {} across {} resource groups",
        p.resource_count, p.resource_groups
    );

    if !p.services.is_empty() {
        println!("  Top services:");
        for c in p.services.iter().take(8) {
            let short = c.name.rsplit('/').next().unwrap_or(&c.name);
            println!("    {:>4}  {short}", c.count);
        }
    }
    if !p.regions.is_empty() {
        let regions = p
            .regions
            .iter()
            .map(|c| format!("{} ({})", c.name, c.count))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  Regions: {regions}");
    }
    println!(
        "  CAF naming: {}% conform ({}/{})",
        p.naming.percent, p.naming.conforming, p.naming.total
    );
    let tags = p
        .tagging
        .iter()
        .map(|t| format!("{} {}%", t.tag, t.percent))
        .collect::<Vec<_>>()
        .join(", ");
    println!("  Required tags: {tags}");
    println!(
        "  Governance: {} policy assignment(s), {} non-compliant",
        p.governance.assignments, p.governance.non_compliant
    );

    if !p.signals.is_empty() {
        println!("\nSignals:");
        for s in &p.signals {
            println!("  • {s}");
        }
    }
    if p.warnings.is_empty() {
        println!("\n✓ No guardrail warnings.");
    } else {
        println!("\nWarnings:");
        for w in &p.warnings {
            println!("  ⚠ {w}");
        }
    }
    println!(
        "\n(Run `raz advise --summary` for a natural-language review once the LLM advisor ships.)"
    );
}
