//! `raz resource ...` — generic CRUD over any resource type/id. Mirrors `az resource`.

use clap::Subcommand;

use raz_core::arm::resource;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum ResourceCommand {
    /// List resources in the subscription, or a resource group, optionally by type.
    List {
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
        /// Filter by resource type, e.g. `Microsoft.Compute/virtualMachines`.
        #[arg(long)]
        resource_type: Option<String>,
    },
    /// Show a resource by its full id (requires the resource's API version).
    Show {
        #[arg(long)]
        ids: String,
        #[arg(long)]
        api_version: String,
    },
    /// Delete a resource by its full id (requires the resource's API version).
    Delete {
        #[arg(long)]
        ids: String,
        #[arg(long)]
        api_version: String,
    },
}

pub async fn run(command: ResourceCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        ResourceCommand::List {
            resource_group,
            resource_type,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = resource::list(
                &client,
                &sub,
                resource_group.as_deref(),
                resource_type.as_deref(),
            )
            .await?;
            emit(&ctx, value, Some(&resource::table_spec()))
        }
        ResourceCommand::Show { ids, api_version } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let value = resource::show(&client, &ids, &api_version).await?;
            emit(&ctx, value, Some(&resource::table_spec()))
        }
        ResourceCommand::Delete { ids, api_version } => {
            let (_ctx, client, _sub) = arm_context(globals).await?;
            eprintln!("Deleting resource…");
            resource::delete(&client, &ids, &api_version).await?;
            println!("Deleted resource.");
            Ok(())
        }
    }
}
