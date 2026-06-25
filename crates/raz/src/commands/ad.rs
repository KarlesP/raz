//! `raz ad ...` — Entra directory commands: `app federated-credential` (create/list/delete) and
//! `sp create-for-rbac` (app + service principal + secret + RBAC role assignment). Runs against
//! Microsoft Graph, with the role assignment via ARM.

use std::time::Duration;

use clap::Subcommand;
use serde_json::json;

use raz_core::arm::client::ArmClient;
use raz_core::arm::role;
use raz_core::context::new_http_client;
use raz_core::error::{RazError, Result};
use raz_core::graph::client::GraphClient;
use raz_core::graph::{federated_credential as fic, service_principal as sp};
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
    /// Manage service principals.
    Sp {
        #[command(subcommand)]
        command: SpCommand,
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
pub enum SpCommand {
    /// Create an app + service principal + secret and assign it an RBAC role.
    CreateForRbac {
        /// Display name for the app registration.
        #[arg(long, short = 'n')]
        name: String,
        /// Role to assign (name or GUID).
        #[arg(long, default_value = "Contributor")]
        role: String,
        /// Scope for the assignment (default: the active subscription).
        #[arg(long)]
        scope: Option<String>,
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
    let ctx = Context::load(globals)?;
    match command {
        AdCommand::App {
            command: AppCommand::FederatedCredential { command },
        } => fed_cred(&ctx, command).await,
        AdCommand::Sp {
            command: SpCommand::CreateForRbac { name, role, scope },
        } => create_for_rbac(&ctx, &name, &role, scope).await,
    }
}

async fn fed_cred(ctx: &Context, command: FedCredCommand) -> Result<()> {
    let token = ctx.graph_token().await?;
    let client = GraphClient::new(new_http_client(), token, ctx.cloud().graph_base())
        .trace(ctx.globals.debug);
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
            emit(ctx, value, Some(&fic::table_spec()))
        }
        FedCredCommand::List { id } => {
            let value = fic::list(&client, &id).await?;
            emit(ctx, value, Some(&fic::table_spec()))
        }
        FedCredCommand::Delete { id, name } => {
            fic::delete(&client, &id, &name).await?;
            println!("Deleted federated credential '{name}'.");
            Ok(())
        }
    }
}

async fn create_for_rbac(
    ctx: &Context,
    name: &str,
    role: &str,
    scope: Option<String>,
) -> Result<()> {
    // 1. App + service principal + secret (Graph).
    let graph_token = ctx.graph_token().await?;
    let graph = GraphClient::new(new_http_client(), graph_token, ctx.cloud().graph_base())
        .trace(ctx.globals.debug);
    let created = sp::create_with_secret(&graph, name).await?;
    let object_id = created["objectId"].as_str().unwrap_or_default().to_string();
    let app_id = created["appId"].as_str().unwrap_or_default().to_string();
    let password = created["password"].as_str().unwrap_or_default().to_string();

    // 2. RBAC role assignment (ARM).
    let (sub_id, token) = ctx.subscription_and_token().await?;
    let tenant = ctx
        .active_subscription()
        .map(|s| s.tenant_id.clone())
        .unwrap_or_default();
    let scope = scope.unwrap_or_else(|| format!("/subscriptions/{sub_id}"));
    let arm = ArmClient::with_token(new_http_client(), token)
        .endpoint(ctx.cloud().arm)
        .trace(ctx.globals.debug);

    // A freshly created principal is eventually consistent; retry the assignment briefly.
    let mut attempt = 0;
    loop {
        match role::create_assignment(
            &arm,
            &sub_id,
            &scope,
            role,
            &object_id,
            Some("ServicePrincipal"),
        )
        .await
        {
            Ok(_) => break,
            Err(_) if attempt < 5 => {
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => return Err(RazError::Other(format!("role assignment failed: {e}"))),
        }
    }

    let out = json!({
        "appId": app_id,
        "displayName": name,
        "password": password,
        "tenant": tenant,
        "scope": scope,
        "role": role,
    });
    emit(
        ctx,
        out,
        Some(&vec![
            ("AppId", "appId"),
            ("Password", "password"),
            ("Tenant", "tenant"),
        ]),
    )
}
