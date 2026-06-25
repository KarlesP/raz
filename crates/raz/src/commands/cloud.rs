//! `raz cloud ...` — view or set the active Azure cloud (public / Gov / China). The selection is
//! persisted in `~/.raz` and threaded through every endpoint, authority, and auth scope.

use clap::Subcommand;
use serde_json::{json, Value};

use raz_core::config::Profile;
use raz_core::context::Context;
use raz_core::error::{usage, Result};
use raz_core::{cloud, GlobalArgs};

use super::emit;

#[derive(Subcommand)]
pub enum CloudCommand {
    /// List the known clouds.
    List,
    /// Show the active cloud.
    Show,
    /// Set the active cloud (AzureCloud / AzureUSGovernment / AzureChinaCloud).
    Set {
        #[arg(long, short = 'n')]
        name: String,
    },
}

fn table() -> raz_core::TableSpec {
    vec![("Name", "name"), ("ARM", "arm"), ("Authority", "authority")]
}

fn describe(c: &cloud::Cloud) -> Value {
    json!({ "name": c.name, "arm": c.arm, "authority": c.authority, "graph": c.graph, "vault": c.vault_suffix })
}

pub fn run(command: CloudCommand, globals: GlobalArgs) -> Result<()> {
    let ctx = Context::load(globals)?;
    match command {
        CloudCommand::List => {
            let rows: Vec<Value> = cloud::all().iter().map(describe).collect();
            emit(&ctx, Value::Array(rows), Some(&table()))
        }
        CloudCommand::Show => emit(&ctx, describe(ctx.cloud()), Some(&table())),
        CloudCommand::Set { name } => {
            let target = cloud::by_name(&name).ok_or_else(|| {
                usage(format!(
                    "unknown cloud '{name}' (AzureCloud | AzureUSGovernment | AzureChinaCloud)"
                ))
            })?;
            let mut profile = Profile::load()?;
            profile.cloud = Some(target.name.to_string());
            profile.save()?;
            println!(
                "Active cloud set to {}. Run `raz login` if you switched clouds.",
                target.name
            );
            Ok(())
        }
    }
}
