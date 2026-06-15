//! `raz login` — device-code flow + subscription discovery + profile persistence.
//! Mirrors az `profile/custom.py::login`.

use clap::Args;

use raz_core::arm::client::discover_all;
use raz_core::auth::{credential, device_code};
use raz_core::config::Profile;
use raz_core::context::new_http_client;
use raz_core::error::Result;
use raz_core::GlobalArgs;

#[derive(Args)]
pub struct LoginArgs {
    /// Microsoft Entra tenant (ID or domain). If omitted, raz uses `AZURE_TENANT_ID`, then
    /// the last tenant you logged into, then the multi-tenant authority (`organizations`).
    #[arg(long, short = 't', env = "AZURE_TENANT_ID")]
    pub tenant: Option<String>,
}

pub async fn run(args: LoginArgs, _globals: &GlobalArgs) -> Result<()> {
    let http = new_http_client();
    let mut profile = Profile::load()?;

    // Resolve the tenant: explicit flag / AZURE_TENANT_ID, else the last tenant used, else
    // the multi-tenant authority. ARM `/subscriptions` is tenant-scoped, so reusing the last
    // tenant means a bare `raz login` keeps seeing the same subscriptions.
    let tenant = args
        .tenant
        .clone()
        .or_else(|| profile.tenant_id.clone())
        .unwrap_or_else(|| "organizations".to_string());
    println!("Signing in to tenant: {tenant}");

    // Step 1+2: device-code flow. The closure prints the user prompt as soon as it's known.
    let token = device_code::run_flow(&http, &tenant, |dc| {
        println!("{}", dc.message);
    })
    .await?;

    // Persist the token first so it can be reused for discovery and later commands.
    profile.tenant_id = Some(tenant);
    profile.token = Some(credential::cache_from_response(&token));
    profile.save()?;

    // Step 3: enumerate every tenant the identity can reach and the subscriptions in each
    // (silently, via the refresh token) — the az-style cross-tenant view.
    let (tenants, subs) = discover_all(&http, &token).await?;

    if !tenants.is_empty() {
        println!("\nAvailable tenants ({}):", tenants.len());
        for t in &tenants {
            let label = if t.display_name.is_empty() {
                t.default_domain.clone()
            } else {
                t.display_name.clone()
            };
            println!("  - {label} ({})", t.id);
        }
    }

    let count = subs.len();
    profile.subscriptions = subs;
    profile.save()?;

    if count == 0 {
        println!("\nLogged in, but no subscriptions were found for this identity.");
    } else {
        println!("\nSubscriptions ({count}):");
        for s in &profile.subscriptions {
            let marker = if s.is_default { "*" } else { " " };
            println!("  {marker} {}  ({})  tenant={}", s.name, s.id, s.tenant_id);
        }
        println!("\nSwitch the active subscription with `raz account set -s <id|name>`,");
        println!("or target one per command with `raz -s <id|name> vm list`.");
    }
    Ok(())
}
