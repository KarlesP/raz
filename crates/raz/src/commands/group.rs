//! `raz group ...` — resource group commands. Mirrors az's `group` group.

use clap::Subcommand;

use raz_core::arm::client::DEFAULT_LOCATION;
use raz_core::arm::group;
use raz_core::error::{usage, Result};
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum GroupCommand {
    /// List resource groups in the subscription.
    List,
    /// Show a single resource group.
    Show {
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Create a resource group (defaults to West Europe).
    Create {
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long, short = 'l', default_value = DEFAULT_LOCATION)]
        location: String,
    },
    /// Delete a resource group and everything in it. Requires --yes to confirm.
    Delete {
        #[arg(long, short = 'n')]
        name: String,
        /// Confirm the (irreversible) deletion of the group and all its resources.
        #[arg(long)]
        yes: bool,
    },
}

pub async fn run(command: GroupCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        GroupCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = group::list(&client, &sub).await?;
            emit(&ctx, value, Some(&group::table_spec()))
        }
        GroupCommand::Show { name } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = group::show(&client, &sub, &name).await?;
            emit(&ctx, value, Some(&group::table_spec()))
        }
        GroupCommand::Create { name, location } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            super::print_caf_recommendation("rg", &name, &location);
            let value = group::create(&client, &sub, &name, &location).await?;
            emit(&ctx, value, Some(&group::table_spec()))
        }
        GroupCommand::Delete { name, yes } => {
            if !yes {
                return Err(usage(format!(
                    "deleting resource group '{name}' removes all resources in it; pass --yes to confirm"
                )));
            }
            let (_ctx, client, sub) = arm_context(globals).await?;
            eprintln!("Deleting resource group '{name}' and all its resources…");
            group::delete(&client, &sub, &name).await?;
            println!("Deleted resource group '{name}'.");
            Ok(())
        }
    }
}
