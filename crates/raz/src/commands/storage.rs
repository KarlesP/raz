//! `raz storage ...` — storage accounts and blob containers (ARM management plane).

use clap::Subcommand;

use raz_core::arm::storage;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum StorageCommand {
    /// Manage storage accounts.
    Account {
        #[command(subcommand)]
        command: AccountCommand,
    },
    /// Manage blob containers.
    Container {
        #[command(subcommand)]
        command: ContainerCommand,
    },
}

#[derive(Subcommand)]
pub enum AccountCommand {
    /// List storage accounts in the subscription.
    List,
    /// Show a storage account.
    Show {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Create a storage account (defaults to Standard_LRS / StorageV2).
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long, short = 'l')]
        location: Option<String>,
        #[arg(long, default_value = "Standard_LRS")]
        sku: String,
        #[arg(long, default_value = "StorageV2")]
        kind: String,
    },
}

#[derive(Subcommand)]
pub enum ContainerCommand {
    /// List blob containers in an account.
    List {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long)]
        account_name: String,
    },
    /// Create a blob container.
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long)]
        account_name: String,
        #[arg(long, short = 'n')]
        name: String,
    },
}

pub async fn run(command: StorageCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        StorageCommand::Account { command } => account(command, globals).await,
        StorageCommand::Container { command } => container(command, globals).await,
    }
}

async fn account(command: AccountCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        AccountCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = storage::list_accounts(&client, &sub).await?;
            emit(&ctx, value, Some(&storage::account_table_spec()))
        }
        AccountCommand::Show {
            resource_group,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = storage::show_account(&client, &sub, &resource_group, &name).await?;
            emit(&ctx, value, Some(&storage::account_table_spec()))
        }
        AccountCommand::Create {
            resource_group,
            name,
            location,
            sku,
            kind,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let location = ctx.resolve_location(location);
            eprintln!("Creating storage account '{name}' in {location}…");
            let value = storage::create_account(
                &client,
                &sub,
                &resource_group,
                &name,
                &location,
                &sku,
                &kind,
            )
            .await?;
            emit(&ctx, value, Some(&storage::account_table_spec()))
        }
    }
}

async fn container(command: ContainerCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        ContainerCommand::List {
            resource_group,
            account_name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value =
                storage::list_containers(&client, &sub, &resource_group, &account_name).await?;
            emit(&ctx, value, Some(&storage::container_table_spec()))
        }
        ContainerCommand::Create {
            resource_group,
            account_name,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value =
                storage::create_container(&client, &sub, &resource_group, &account_name, &name)
                    .await?;
            emit(&ctx, value, Some(&storage::container_table_spec()))
        }
    }
}
