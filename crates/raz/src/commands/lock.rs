//! `raz lock ...` — management locks (CanNotDelete / ReadOnly) at subscription, resource-group,
//! or resource scope.

use clap::Subcommand;

use raz_core::arm::lock;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum LockCommand {
    /// Create a lock at a scope (subscription, -g resource group, or --resource id).
    Create {
        #[arg(long, short = 'n')]
        name: String,
        /// CanNotDelete or ReadOnly.
        #[arg(long)]
        lock_type: String,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
        #[arg(long)]
        resource: Option<String>,
        #[arg(long)]
        notes: Option<String>,
    },
    /// List locks at a scope.
    List {
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
        #[arg(long)]
        resource: Option<String>,
    },
    /// Delete a lock by name at a scope.
    Delete {
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
        #[arg(long)]
        resource: Option<String>,
    },
}

pub async fn run(command: LockCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        LockCommand::Create {
            name,
            lock_type,
            resource_group,
            resource,
            notes,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let scope = lock::scope(&sub, resource_group.as_deref(), resource.as_deref());
            let value = lock::create(&client, &scope, &name, &lock_type, notes.as_deref()).await?;
            emit(&ctx, value, Some(&lock::table_spec()))
        }
        LockCommand::List {
            resource_group,
            resource,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let scope = lock::scope(&sub, resource_group.as_deref(), resource.as_deref());
            let value = lock::list(&client, &scope).await?;
            emit(&ctx, value, Some(&lock::table_spec()))
        }
        LockCommand::Delete {
            name,
            resource_group,
            resource,
        } => {
            let (_ctx, client, sub) = arm_context(globals).await?;
            let scope = lock::scope(&sub, resource_group.as_deref(), resource.as_deref());
            lock::delete(&client, &scope, &name).await?;
            println!("Deleted lock '{name}'.");
            Ok(())
        }
    }
}
