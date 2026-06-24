//! CLI command implementations, one module per command group — the front-end analogue of
//! az's per-module `custom.py`. Shared rendering helpers live here.

pub mod account;
pub mod ad;
pub mod group;
pub mod login;
pub mod logout;
pub mod rest;
pub mod vm;
pub mod vnet;

use serde_json::Value;

use raz_core::context::Context;
use raz_core::error::{usage, Result};
use raz_core::output::{self, TableSpec};
use raz_core::GlobalArgs;

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
