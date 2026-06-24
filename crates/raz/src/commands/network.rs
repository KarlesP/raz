//! `raz network ...` — NSGs, public IPs, and NICs (list/show). Mirrors az's `network` group.

use clap::Subcommand;

use raz_core::arm::{network, resource_table_spec};
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum NetworkCommand {
    /// Network security groups.
    Nsg {
        #[command(subcommand)]
        command: NetResourceCommand,
    },
    /// Public IP addresses.
    PublicIp {
        #[command(subcommand)]
        command: NetResourceCommand,
    },
    /// Network interfaces.
    Nic {
        #[command(subcommand)]
        command: NetResourceCommand,
    },
}

#[derive(Subcommand)]
pub enum NetResourceCommand {
    /// List in the subscription.
    List,
    /// Show one by resource group + name.
    Show {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
    },
}

pub async fn run(command: NetworkCommand, globals: GlobalArgs) -> Result<()> {
    let (segment, command) = match command {
        NetworkCommand::Nsg { command } => ("networkSecurityGroups", command),
        NetworkCommand::PublicIp { command } => ("publicIPAddresses", command),
        NetworkCommand::Nic { command } => ("networkInterfaces", command),
    };
    let (ctx, client, sub) = arm_context(globals).await?;
    let value = match command {
        NetResourceCommand::List => network::list(&client, &sub, segment).await?,
        NetResourceCommand::Show {
            resource_group,
            name,
        } => network::show(&client, &sub, &resource_group, segment, &name).await?,
    };
    emit(&ctx, value, Some(&resource_table_spec()))
}
