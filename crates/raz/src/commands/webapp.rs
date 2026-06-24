//! `raz webapp ...` — App Service web apps (list/show/create).

use clap::Subcommand;

use raz_core::arm::appservice;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum WebappCommand {
    /// List web apps in the subscription.
    List,
    /// Show a web app.
    Show {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Create a web app bound to an existing App Service plan.
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        /// Name of an existing App Service plan (same resource group).
        #[arg(long, short = 'p')]
        plan: String,
        /// Linux runtime stack, e.g. "NODE|18-lts", "PYTHON|3.12".
        #[arg(long)]
        runtime: Option<String>,
    },
}

pub async fn run(command: WebappCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        WebappCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = appservice::list_webapps(&client, &sub).await?;
            emit(&ctx, value, Some(&appservice::webapp_table_spec()))
        }
        WebappCommand::Show {
            resource_group,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = appservice::show_webapp(&client, &sub, &resource_group, &name).await?;
            emit(&ctx, value, Some(&appservice::webapp_table_spec()))
        }
        WebappCommand::Create {
            resource_group,
            name,
            plan,
            runtime,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            eprintln!("Creating web app '{name}'…");
            let value = appservice::create_webapp(
                &client,
                &sub,
                &resource_group,
                &name,
                &plan,
                runtime.as_deref(),
            )
            .await?;
            emit(&ctx, value, Some(&appservice::webapp_table_spec()))
        }
    }
}
