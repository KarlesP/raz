//! `raz account ...` — inspect and switch the active subscription / view tenants.
//! Mirrors az's `account` command group (`list`, `show`, `set`).

use clap::Subcommand;
use serde_json::{json, Value};

use raz_core::auth::{device_code, now_unix};
use raz_core::config::Profile;
use raz_core::context::Context;
use raz_core::error::{usage, Result};
use raz_core::output::TableSpec;
use raz_core::{GlobalArgs, RazError};

use super::emit;

#[derive(Subcommand)]
pub enum AccountCommand {
    /// List the subscriptions cached at login (across all tenants).
    List,
    /// Show the active (default) subscription.
    Show,
    /// Set the active subscription by ID or name, persisted for later commands.
    Set {
        /// Subscription ID or name. Use the ID when names are not unique.
        #[arg(long, short = 's')]
        subscription: String,
    },
    /// List the distinct tenants the cached subscriptions belong to.
    ListTenants,
    /// Print a bearer token (and expiry) for the active subscription's tenant, for scripting/CI.
    GetAccessToken,
}

fn subscription_table() -> TableSpec {
    vec![
        ("Name", "name"),
        ("SubscriptionId", "id"),
        ("Default", "is_default"),
        ("TenantId", "tenant_id"),
    ]
}

pub async fn run(command: AccountCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        AccountCommand::List => {
            let ctx = Context::load(globals)?;
            let value = serde_json::to_value(&ctx.profile.subscriptions)?;
            emit(&ctx, value, Some(&subscription_table()))
        }
        AccountCommand::Show => {
            let ctx = Context::load(globals)?;
            let value = match ctx.active_subscription() {
                Some(s) => serde_json::to_value(s)?,
                None => Value::Null,
            };
            emit(&ctx, value, Some(&subscription_table()))
        }
        AccountCommand::ListTenants => {
            let ctx = Context::load(globals)?;
            // Distinct tenants from the cached subscriptions, preserving first-seen order.
            let mut seen = Vec::new();
            for s in &ctx.profile.subscriptions {
                if !s.tenant_id.is_empty() && !seen.contains(&s.tenant_id) {
                    seen.push(s.tenant_id.clone());
                }
            }
            let value = Value::Array(seen.into_iter().map(|t| json!({ "tenantId": t })).collect());
            emit(&ctx, value, Some(&vec![("TenantId", "tenantId")]))
        }
        AccountCommand::Set { subscription } => set_default(subscription),
        AccountCommand::GetAccessToken => {
            let ctx = Context::load(globals)?;
            let (sub_id, tenant) = ctx
                .active_subscription()
                .map(|s| (s.id.clone(), s.tenant_id.clone()))
                .ok_or(RazError::NotLoggedIn)?;
            let cached = ctx.profile.token.as_ref().ok_or(RazError::NotLoggedIn)?;
            // Prefer a fresh tenant-scoped token from the refresh token; else the cached one.
            let (access_token, expires_on) = match &cached.refresh_token {
                Some(refresh) => {
                    let tok = device_code::exchange_refresh_token(
                        &ctx.http,
                        &tenant,
                        refresh,
                        device_code::DEFAULT_SCOPE,
                    )
                    .await?;
                    (tok.access_token, now_unix() + tok.expires_in)
                }
                None => (cached.access_token.clone(), cached.expires_on),
            };
            let value = json!({
                "tokenType": "Bearer",
                "accessToken": access_token,
                "expiresOn": expires_on,
                "subscription": sub_id,
                "tenant": tenant,
            });
            emit(&ctx, value, None)
        }
    }
}

/// Persist the chosen subscription as default in `~/.raz`. Errors if the name is ambiguous.
fn set_default(want: String) -> Result<()> {
    let mut profile = Profile::load()?;

    let matches: Vec<usize> = profile
        .subscriptions
        .iter()
        .enumerate()
        .filter(|(_, s)| s.id == want || s.name == want)
        .map(|(i, _)| i)
        .collect();

    match matches.as_slice() {
        [] => Err(usage(format!(
            "subscription '{want}' not found; run `raz account list`"
        ))),
        [idx] => {
            let idx = *idx;
            for s in &mut profile.subscriptions {
                s.is_default = false;
            }
            profile.subscriptions[idx].is_default = true;
            profile.save()?;
            let s = &profile.subscriptions[idx];
            println!("Active subscription set to {} ({}).", s.name, s.id);
            Ok(())
        }
        _ => Err(usage(format!(
            "'{want}' matches {} subscriptions by name; pass the subscription ID instead",
            matches.len()
        ))),
    }
}
