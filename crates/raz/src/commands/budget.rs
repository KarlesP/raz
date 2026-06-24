//! `raz budget ...` — cost-management budgets at subscription or resource-group scope.

use clap::Subcommand;

use raz_core::arm::{budget, role};
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum BudgetCommand {
    /// Create a cost budget at the subscription (or -g resource group) scope.
    Create {
        #[arg(long, short = 'n')]
        name: String,
        /// Budget amount (in the billing currency).
        #[arg(long)]
        amount: f64,
        /// Monthly, Quarterly, or Annually.
        #[arg(long, default_value = "Monthly")]
        time_grain: String,
        /// Start date, ISO (e.g. 2026-07-01T00:00:00Z).
        #[arg(long)]
        start_date: String,
        /// Optional end date, ISO.
        #[arg(long)]
        end_date: Option<String>,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
    },
    /// Update an existing budget's amount / time-grain / end-date.
    Update {
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long)]
        amount: Option<f64>,
        #[arg(long)]
        time_grain: Option<String>,
        #[arg(long)]
        end_date: Option<String>,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
    },
    /// List budgets at a scope.
    List {
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
    },
    /// Delete a budget by name.
    Delete {
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
    },
}

pub async fn run(command: BudgetCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        BudgetCommand::Create {
            name,
            amount,
            time_grain,
            start_date,
            end_date,
            resource_group,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let scope = role::scope(&sub, resource_group.as_deref());
            let value = budget::create(
                &client,
                &scope,
                &name,
                amount,
                &time_grain,
                &start_date,
                end_date.as_deref(),
            )
            .await?;
            emit(&ctx, value, Some(&budget::table_spec()))
        }
        BudgetCommand::Update {
            name,
            amount,
            time_grain,
            end_date,
            resource_group,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let scope = role::scope(&sub, resource_group.as_deref());
            let value = budget::update(
                &client,
                &scope,
                &name,
                amount,
                time_grain.as_deref(),
                end_date.as_deref(),
            )
            .await?;
            emit(&ctx, value, Some(&budget::table_spec()))
        }
        BudgetCommand::List { resource_group } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let scope = role::scope(&sub, resource_group.as_deref());
            let value = budget::list(&client, &scope).await?;
            emit(&ctx, value, Some(&budget::table_spec()))
        }
        BudgetCommand::Delete {
            name,
            resource_group,
        } => {
            let (_ctx, client, sub) = arm_context(globals).await?;
            let scope = role::scope(&sub, resource_group.as_deref());
            budget::delete(&client, &scope, &name).await?;
            println!("Deleted budget '{name}'.");
            Ok(())
        }
    }
}
