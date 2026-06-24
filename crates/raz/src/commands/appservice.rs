//! `raz appservice ...` — App Service plans.

use clap::Subcommand;

use raz_core::arm::appservice;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum AppserviceCommand {
    /// Manage App Service plans.
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
}

#[derive(Subcommand)]
pub enum PlanCommand {
    /// Create an App Service plan.
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long, short = 'l', default_value = "westeurope")]
        location: String,
        /// SKU, e.g. B1, S1, P1v3, F1.
        #[arg(long, default_value = "B1")]
        sku: String,
        /// Create a Linux plan.
        #[arg(long)]
        is_linux: bool,
    },
}

pub async fn run(command: AppserviceCommand, globals: GlobalArgs) -> Result<()> {
    let AppserviceCommand::Plan {
        command:
            PlanCommand::Create {
                resource_group,
                name,
                location,
                sku,
                is_linux,
            },
    } = command;
    let (ctx, client, sub) = arm_context(globals).await?;
    eprintln!("Creating App Service plan '{name}' in {location}…");
    let value = appservice::create_plan(
        &client,
        &sub,
        &resource_group,
        &name,
        &location,
        &sku,
        is_linux,
    )
    .await?;
    emit(&ctx, value, Some(&appservice::plan_table_spec()))
}
