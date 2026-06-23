//! `raz ad ...` — Entra directory commands. Currently `app federated-credential` (create/list/
//! delete), mirroring `az ad app federated-credential`. Runs against Microsoft Graph.

use clap::Subcommand;

use raz_core::context::new_http_client;
use raz_core::error::Result;
use raz_core::graph::client::GraphClient;
use raz_core::graph::federated_credential as fic;
use raz_core::Context;
use raz_core::GlobalArgs;

use super::emit;

#[derive(Subcommand)]
pub enum AdCommand {
    /// Manage app registrations.
    App {
        #[command(subcommand)]
        command: AppCommand,
    },
}

#[derive(Subcommand)]
pub enum AppCommand {
    /// Manage an app registration's federated identity credentials (OIDC trust).
    FederatedCredential {
        #[command(subcommand)]
        command: FedCredCommand,
    },
}

#[derive(Subcommand)]
pub enum FedCredCommand {
    /// Add a federated identity credential to an app registration.
    Create {
        /// App registration appId (client id) or object id.
        #[arg(long)]
        id: String,
        /// Credential name.
        #[arg(long, short = 'n')]
        name: String,
        /// Issuer URL (e.g. https://token.actions.githubusercontent.com).
        #[arg(long)]
        issuer: String,
        /// Subject (e.g. repo:OWNER/REPO:ref:refs/heads/main).
        #[arg(long)]
        subject: String,
        /// Token audience.
        #[arg(long, default_value = "api://AzureADTokenExchange")]
        audience: String,
        /// Optional description.
        #[arg(long)]
        description: Option<String>,
    },
    /// List an app registration's federated identity credentials.
    List {
        #[arg(long)]
        id: String,
    },
    /// Delete a federated identity credential by name.
    Delete {
        #[arg(long)]
        id: String,
        #[arg(long, short = 'n')]
        name: String,
    },
}

pub async fn run(command: AdCommand, globals: GlobalArgs) -> Result<()> {
    let AdCommand::App { command } = command;
    let AppCommand::FederatedCredential { command } = command;

    // Graph token comes from the interactive session; build a client once.
    let ctx = Context::load(globals)?;
    let client = GraphClient::new(new_http_client(), ctx.graph_token().await?);

    match command {
        FedCredCommand::Create {
            id,
            name,
            issuer,
            subject,
            audience,
            description,
        } => {
            let value = fic::create(
                &client,
                &id,
                &name,
                &issuer,
                &subject,
                &audience,
                description.as_deref(),
            )
            .await?;
            emit(&ctx, value, Some(&fic::table_spec()))
        }
        FedCredCommand::List { id } => {
            let value = fic::list(&client, &id).await?;
            emit(&ctx, value, Some(&fic::table_spec()))
        }
        FedCredCommand::Delete { id, name } => {
            fic::delete(&client, &id, &name).await?;
            println!("Deleted federated credential '{name}'.");
            Ok(())
        }
    }
}
