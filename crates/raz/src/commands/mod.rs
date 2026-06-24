//! CLI command implementations, one module per command group — the front-end analogue of
//! az's per-module `custom.py`. Shared rendering helpers live here.

pub mod account;
pub mod ad;
pub mod aks;
pub mod budget;
pub mod completion;
pub mod deployment;
pub mod group;
pub mod keyvault;
pub mod lock;
pub mod login;
pub mod logout;
pub mod network;
pub mod policy;
pub mod resource;
pub mod rest;
pub mod role;
pub mod storage;
pub mod subscription;
pub mod suggest;
pub mod tag;
pub mod vm;
pub mod vnet;

use serde_json::Value;

use raz_core::context::Context;
use raz_core::error::{usage, Result};
use raz_core::output::{self, TableSpec};
use raz_core::suggest as caf;
use raz_core::GlobalArgs;

/// Print the CAF naming + ALZ tagging recommendation to stderr before a create, automatically.
/// Informational only (stderr keeps stdout/JSON clean); shows the recommended scheme, a concrete
/// example for the target region, and the tag set to apply.
pub(crate) fn print_caf_recommendation(kind: &str, name: &str, location: &str) {
    let region = caf::region_abbrev(location);
    let example = caf::suggest_name(kind, "<workload>", "<env>", &region, "001");
    let required: Vec<&str> = caf::recommended_tags()
        .iter()
        .filter(|t| t.required)
        .map(|t| t.key)
        .collect();
    let tag_args = required
        .iter()
        .map(|k| format!("--tag {k}=<{k}>"))
        .collect::<Vec<_>>()
        .join(" ");

    eprintln!("\nRecommended based on Microsoft Azure CAF");
    eprintln!("  Naming scheme : {}", caf::NAME_PATTERN);
    eprintln!("  Example       : {example}");
    eprintln!("  You chose     : {name}");
    eprintln!("  Required tags : {}", required.join(", "));
    eprintln!("  Optional tags : application, department");
    eprintln!("  Apply tags    : {tag_args}\n");
}

/// Parse repeated `key=value` `--tag` arguments into pairs.
pub(crate) fn parse_tags(pairs: &[String]) -> Result<Vec<(String, String)>> {
    pairs
        .iter()
        .map(|p| {
            p.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| usage(format!("invalid --tag '{p}', expected key=value")))
        })
        .collect()
}

/// Render a command result to stdout honoring the global `--output` and `--query`,
/// matching how az pipes every result through its output system.
pub(crate) fn emit(ctx: &Context, value: Value, table: Option<&TableSpec>) -> Result<()> {
    // `--output none`: suppress result output entirely (az parity).
    if matches!(ctx.globals.output, raz_core::OutputFormat::None) {
        return Ok(());
    }
    let projected = match &ctx.globals.query {
        Some(q) => output::apply_query(&value, q),
        None => value,
    };
    let text = output::render(&projected, ctx.globals.output, table)?;
    println!("{text}");
    Ok(())
}

/// Build the common preamble for data commands: a context, an ARM client authenticated with
/// a token minted for the target subscription's tenant, and the resolved subscription id.
pub(crate) async fn arm_context(
    globals: GlobalArgs,
) -> Result<(Context, raz_core::arm::client::ArmClient, String)> {
    let ctx = Context::load(globals)?;
    let (subscription, token) = ctx.subscription_and_token().await?;
    let client = raz_core::arm::client::ArmClient::with_token(ctx.http.clone(), token);
    Ok((ctx, client, subscription))
}
