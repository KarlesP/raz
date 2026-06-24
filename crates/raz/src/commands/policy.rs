//! `raz policy ...` — governance scanning. `validation` lists policy assignments joined with
//! compliance, tiered by scope (subscription → resource group), optionally filtered to a service.

use clap::Subcommand;

use raz_core::arm::policy;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum PolicyCommand {
    /// Scan governance: policy assignments + compliance, tiered by scope.
    Validation {
        /// Service to filter by (vnet, vm, storage, keyvault, sql, aks, tag). Omit for everything.
        service: Option<String>,
        /// Scan all policies (no service filter).
        #[arg(long)]
        all: bool,
    },
}

pub async fn run(command: PolicyCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        PolicyCommand::Validation { service, all } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            // `--all` (or no service) clears the filter.
            let filter = if all { None } else { service.as_deref() };
            let value = policy::scan(&client, &sub, filter).await?;
            emit(&ctx, value, Some(&policy::table_spec()))
        }
    }
}
