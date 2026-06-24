//! `raz wait` — block until a resource reaches a condition (az `... wait`).

use clap::Args;

use raz_core::arm::wait::{self, WaitFor};
use raz_core::error::{usage, Result};
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Args)]
pub struct WaitArgs {
    /// Full resource id to poll.
    #[arg(long)]
    ids: String,
    /// API version for the resource.
    #[arg(long)]
    api_version: String,
    /// Wait until provisioningState is Succeeded.
    #[arg(long)]
    created: bool,
    /// Wait until the resource is deleted (404).
    #[arg(long)]
    deleted: bool,
    /// Wait until the resource exists.
    #[arg(long)]
    exists: bool,
    /// Wait until a JMESPath expression over the resource is truthy.
    #[arg(long)]
    custom: Option<String>,
    /// Poll interval, seconds.
    #[arg(long, default_value_t = 5)]
    interval: u64,
    /// Overall timeout, seconds.
    #[arg(long, default_value_t = 3600)]
    timeout: u64,
}

pub async fn run(args: WaitArgs, globals: GlobalArgs) -> Result<()> {
    let condition = match (args.created, args.deleted, args.exists, &args.custom) {
        (true, false, false, None) => WaitFor::Created,
        (false, true, false, None) => WaitFor::Deleted,
        (false, false, true, None) => WaitFor::Exists,
        (false, false, false, Some(expr)) => WaitFor::Custom(expr.clone()),
        _ => {
            return Err(usage(
                "pass exactly one of --created | --deleted | --exists | --custom <jmespath>",
            ))
        }
    };
    let (ctx, client, _sub) = arm_context(globals).await?;
    let value = wait::wait(
        &client,
        &args.ids,
        &args.api_version,
        &condition,
        args.interval,
        args.timeout,
    )
    .await?;
    emit(&ctx, value, None)
}
