//! `raz keyvault ...` — vault management (ARM) and secret get/set (Key Vault data plane).

use clap::Subcommand;
use serde_json::{json, Value};

use raz_core::arm::keyvault;
use raz_core::error::{RazError, Result};
use raz_core::output::TableSpec;
use raz_core::{Context, GlobalArgs};

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum KeyvaultCommand {
    /// List key vaults in the subscription.
    List,
    /// Show a key vault.
    Show {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Create an RBAC-authorization key vault.
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long, short = 'l')]
        location: Option<String>,
        #[arg(long, default_value = "standard")]
        sku: String,
    },
    /// Manage secrets (data plane).
    Secret {
        #[command(subcommand)]
        command: SecretCommand,
    },
}

#[derive(Subcommand)]
pub enum SecretCommand {
    /// Set (create or update) a secret value.
    Set {
        #[arg(long)]
        vault_name: String,
        #[arg(long, short = 'n')]
        name: String,
        #[arg(long)]
        value: String,
    },
    /// Show a secret value.
    Show {
        #[arg(long)]
        vault_name: String,
        #[arg(long, short = 'n')]
        name: String,
    },
}

fn secret_table_spec() -> TableSpec {
    vec![("Name", "name"), ("Value", "value")]
}

pub async fn run(command: KeyvaultCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        KeyvaultCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = keyvault::list(&client, &sub).await?;
            emit(&ctx, value, Some(&keyvault::table_spec()))
        }
        KeyvaultCommand::Show {
            resource_group,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = keyvault::show(&client, &sub, &resource_group, &name).await?;
            emit(&ctx, value, Some(&keyvault::table_spec()))
        }
        KeyvaultCommand::Create {
            resource_group,
            name,
            location,
            sku,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let location = ctx.resolve_location(location);
            let tenant = ctx
                .active_subscription()
                .map(|s| s.tenant_id.clone())
                .ok_or(RazError::NotLoggedIn)?;
            eprintln!("Creating key vault '{name}' in {location}…");
            let value = keyvault::create(
                &client,
                &sub,
                &resource_group,
                &name,
                &location,
                &tenant,
                &sku,
            )
            .await?;
            emit(&ctx, value, Some(&keyvault::table_spec()))
        }
        KeyvaultCommand::Secret { command } => secret(command, globals).await,
    }
}

async fn secret(command: SecretCommand, globals: GlobalArgs) -> Result<()> {
    let ctx = Context::load(globals)?;
    let token = ctx.vault_token().await?;
    match command {
        SecretCommand::Set {
            vault_name,
            name,
            value,
        } => {
            let url = secret_url(&vault_name, &name);
            let resp = ctx
                .http
                .put(&url)
                .bearer_auth(&token)
                .json(&json!({ "value": value }))
                .send()
                .await?;
            let body = read_json(resp).await?;
            emit(&ctx, secret_row(&name, &body), Some(&secret_table_spec()))
        }
        SecretCommand::Show { vault_name, name } => {
            let url = secret_url(&vault_name, &name);
            let resp = ctx.http.get(&url).bearer_auth(&token).send().await?;
            let body = read_json(resp).await?;
            emit(&ctx, secret_row(&name, &body), Some(&secret_table_spec()))
        }
    }
}

fn secret_url(vault: &str, name: &str) -> String {
    format!("https://{vault}.vault.azure.net/secrets/{name}?api-version=7.4")
}

fn secret_row(name: &str, body: &Value) -> Value {
    json!({ "name": name, "value": body.get("value").and_then(Value::as_str).unwrap_or("") })
}

async fn read_json(resp: reqwest::Response) -> Result<Value> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(RazError::Http(format!(
            "Key Vault {}: {text}",
            status.as_u16()
        )));
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}
