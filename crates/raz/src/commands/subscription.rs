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
    }
}
