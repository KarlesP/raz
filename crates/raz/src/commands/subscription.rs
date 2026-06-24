//! `raz subscription ...` — create a subscription (alias API) and check its provisioning state.

use clap::Subcommand;

use raz_core::arm::subscription;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum SubscriptionCommand {
    /// Create a subscription under a billing scope (needs an EA/MCA billing role).
    Create {
        /// Alias (handle) for the create request.
        #[arg(long)]
        alias: String,
        #[arg(long)]
        display_name: String,
        /// Billing scope, e.g. /providers/Microsoft.Billing/billingAccounts/.../...
        #[arg(long)]
        billing_scope: String,
        /// Production or DevTest.
        #[arg(long, default_value = "Production")]
        workload: String,
    },
    /// Show a create-alias and its provisioning state.
    Show {
        #[arg(long)]
        alias: String,
    },
    /// Delete a create-alias record (does not cancel the subscription itself).
    Delete {
        #[arg(long)]
        alias: String,
    },
    /// Rename a subscription's display name.
    Rename {
        /// Subscription id to rename.
        #[arg(long)]
        id: String,
        /// New display name.
        #[arg(long)]
        name: String,
    },
}

pub async fn run(command: SubscriptionCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        SubscriptionCommand::Create {
            alias,
            display_name,
            billing_scope,
            workload,
        } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let value =
                subscription::create(&client, &alias, &display_name, &billing_scope, &workload)
                    .await?;
            emit(&ctx, value, Some(&subscription::table_spec()))
        }
        SubscriptionCommand::Show { alias } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let value = subscription::show(&client, &alias).await?;
            emit(&ctx, value, Some(&subscription::table_spec()))
        }
        SubscriptionCommand::Delete { alias } => {
            let (_ctx, client, _sub) = arm_context(globals).await?;
            subscription::delete_alias(&client, &alias).await?;
            println!("Deleted subscription alias '{alias}'.");
            Ok(())
        }
        SubscriptionCommand::Rename { id, name } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let value = subscription::rename(&client, &id, &name).await?;
            emit(&ctx, value, None)
        }
    }
}
