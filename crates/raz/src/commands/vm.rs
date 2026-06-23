//! `raz vm ...` — virtual machine commands. Mirrors az's `vm` group.

use clap::Subcommand;

use raz_core::arm::client::DEFAULT_LOCATION;
use raz_core::arm::vm::{self, VmCreate, DEFAULT_VM_SIZE};
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit, parse_tags};

#[derive(Subcommand)]
pub enum VmCommand {
    /// List virtual machines in the subscription.
    List,
    /// Show a single virtual machine.
    Show {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Create a Linux VM (defaults to West Europe, Ubuntu 22.04, Standard_B1s). Auto-creates a
    /// resource group, virtual network/subnet, and NIC as needed.
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        /// Azure region.
        #[arg(long, short = 'l', default_value = DEFAULT_LOCATION)]
        location: String,
        /// VM size (SKU).
        #[arg(long, default_value = DEFAULT_VM_SIZE)]
        size: String,
        /// Admin username.
        #[arg(long, default_value = "azureuser")]
        admin_username: String,
        /// SSH public key value (preferred). Provide this or --admin-password.
        #[arg(long)]
        ssh_key_value: Option<String>,
        /// Admin password (alternative to --ssh-key-value).
        #[arg(long)]
        admin_password: Option<String>,
    },
    /// Update a VM's size and/or tags.
    Update {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        /// New VM size (SKU) to resize to.
        #[arg(long)]
        size: Option<String>,
        /// Tag in `key=value` form (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Delete a virtual machine.
    Delete {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Start a virtual machine (not yet implemented).
    Start {
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Stop (power off) a virtual machine (not yet implemented).
    Stop {
        #[arg(long, short = 'n')]
        name: String,
    },
}

pub async fn run(command: VmCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        VmCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = vm::list(&client, &sub).await?;
            emit(&ctx, value, Some(&raz_core::arm::resource_table_spec()))
        }
        VmCommand::Show {
            resource_group,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = vm::show(&client, &sub, &resource_group, &name).await?;
            emit(&ctx, value, Some(&raz_core::arm::resource_table_spec()))
        }
        VmCommand::Create {
            resource_group,
            name,
            location,
            size,
            admin_username,
            ssh_key_value,
            admin_password,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            eprintln!("Creating VM '{name}' in {location} (this can take a few minutes)…");
            let value = vm::create(
                &client,
                &VmCreate {
                    subscription: &sub,
                    resource_group: &resource_group,
                    name: &name,
                    location: &location,
                    size: &size,
                    admin_username: &admin_username,
                    ssh_key: ssh_key_value.as_deref(),
                    admin_password: admin_password.as_deref(),
                },
            )
            .await?;
            emit(&ctx, value, Some(&raz_core::arm::resource_table_spec()))
        }
        VmCommand::Update {
            resource_group,
            name,
            size,
            tags,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let tags = parse_tags(&tags)?;
            let value = vm::update(
                &client,
                &sub,
                &resource_group,
                &name,
                size.as_deref(),
                &tags,
            )
            .await?;
            emit(&ctx, value, Some(&raz_core::arm::resource_table_spec()))
        }
        VmCommand::Delete {
            resource_group,
            name,
        } => {
            let (_ctx, client, sub) = arm_context(globals).await?;
            eprintln!("Deleting VM '{name}'…");
            vm::delete(&client, &sub, &resource_group, &name).await?;
            println!("Deleted VM '{name}'.");
            Ok(())
        }
        VmCommand::Start { name } => vm::start(&name).await.map(|_| ()),
        VmCommand::Stop { name } => vm::stop(&name).await.map(|_| ()),
    }
}
