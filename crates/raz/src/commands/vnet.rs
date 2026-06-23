//! `raz vnet ...` — virtual network commands. Mirrors az's `network vnet` group.

use clap::Subcommand;

use raz_core::arm::client::DEFAULT_LOCATION;
use raz_core::arm::vnet::{self, VnetCreate};
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit, parse_tags};

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
    /// Create a virtual network (defaults to West Europe) with a single subnet.
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        /// Azure region.
        #[arg(long, short = 'l', default_value = DEFAULT_LOCATION)]
        location: String,
        /// VNet address space.
        #[arg(long, default_value = "10.0.0.0/16")]
        address_prefix: String,
        /// Name of the default subnet to create.
        #[arg(long, default_value = "default")]
        subnet_name: String,
        /// Address prefix of the default subnet.
        #[arg(long, default_value = "10.0.0.0/24")]
        subnet_prefix: String,
    },
    /// Update a virtual network's tags and/or add an address prefix.
    Update {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        /// Tag in `key=value` form (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Append an additional address prefix to the VNet address space.
        #[arg(long)]
        add_prefix: Option<String>,
    },
    /// Delete a virtual network.
    Delete {
        #[arg(long, short = 'g')]
        resource_group: String,
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
        VnetCommand::Create {
            resource_group,
            name,
            location,
            address_prefix,
            subnet_name,
            subnet_prefix,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            eprintln!("Creating vnet '{name}' in {location}…");
            let value = vnet::create(
                &client,
                &VnetCreate {
                    subscription: &sub,
                    resource_group: &resource_group,
                    name: &name,
                    location: &location,
                    address_prefix: &address_prefix,
                    subnet_name: &subnet_name,
                    subnet_prefix: &subnet_prefix,
                },
            )
            .await?;
            emit(&ctx, value, Some(&vnet::table_spec()))
        }
        VnetCommand::Update {
            resource_group,
            name,
            tags,
            add_prefix,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let tags = parse_tags(&tags)?;
            let value = vnet::update(
                &client,
                &sub,
                &resource_group,
                &name,
                &tags,
                add_prefix.as_deref(),
            )
            .await?;
            emit(&ctx, value, Some(&vnet::table_spec()))
        }
        VnetCommand::Delete {
            resource_group,
            name,
        } => {
            let (_ctx, client, sub) = arm_context(globals).await?;
            eprintln!("Deleting vnet '{name}'…");
            vnet::delete(&client, &sub, &resource_group, &name).await?;
            println!("Deleted vnet '{name}'.");
            Ok(())
        }
    }
}
