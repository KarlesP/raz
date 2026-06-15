//! `raz vnet ...` — virtual network commands. Mirrors az's `network vnet` group.

use clap::Subcommand;

use raz_core::arm::vnet;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum VnetCommand {
    /// List virtual networks in the subscription.
    List,
    /// Show a single virtual network.
    Show {
        /// Resource group name.
        #[arg(long, short = 'g')]
        resource_group: String,
        /// Virtual network name.
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Create a virtual network (stubbed in this skeleton).
    Create {
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Delete a virtual network (stubbed in this skeleton).
    Delete {
        #[arg(long, short = 'n')]
        name: String,
    },
}

pub async fn run(command: VnetCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        VnetCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = vnet::list(&client, &sub).await?;
            emit(&ctx, value, Some(&vnet::table_spec()))
        }
        VnetCommand::Show {
            resource_group,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = vnet::show(&client, &sub, &resource_group, &name).await?;
            emit(&ctx, value, Some(&vnet::table_spec()))
        }
        VnetCommand::Create { name } => vnet::create(&name).await.map(|_| ()),
        VnetCommand::Delete { name } => vnet::delete(&name).await.map(|_| ()),
    }
}
