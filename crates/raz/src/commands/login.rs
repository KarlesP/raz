//! `raz login` — interactive device-code flow or non-interactive service-principal sign-in
//! (client secret or OIDC federated token), then subscription discovery + profile persistence.
//! Mirrors `az login` / `az login --service-principal`.

use clap::Args;

use raz_core::arm::client::discover_all;
use raz_core::auth::device_code::TokenResponse;
use raz_core::auth::{cache_from_response, device_code, sp};
use raz_core::config::Profile;
use raz_core::context::new_http_client;
use raz_core::error::{usage, Result};
use raz_core::GlobalArgs;

#[derive(Args)]
pub struct LoginArgs {
    /// Microsoft Entra tenant (ID or domain). If omitted, raz uses `AZURE_TENANT_ID`, then
    /// the last tenant you logged into, then the multi-tenant authority (`organizations`).
    #[arg(long, short = 't', env = "AZURE_TENANT_ID")]
    pub tenant: Option<String>,

    /// Log in using managed identity.
    #[arg(long, short = 'i', help_heading = "Managed Identity")]
    pub identity: bool,

    /// Object ID of the user-assigned managed identity.
    #[arg(long, help_heading = "Managed Identity")]
    pub object_id: Option<String>,

    /// Resource ID of the user-assigned managed identity.
    #[arg(long, help_heading = "Managed Identity")]
    pub resource_id: Option<String>,

    /// Sign in as a service principal (non-interactive) instead of the device-code flow.
    #[arg(long)]
    pub service_principal: bool,

    /// Service-principal application (client) ID. Required with --service-principal.
    #[arg(long, env = "AZURE_CLIENT_ID")]
    pub client_id: Option<String>,

    /// Service-principal client secret.
    #[arg(long, env = "AZURE_CLIENT_SECRET")]
    pub client_secret: Option<String>,

    /// OIDC federated token (JWT) for passwordless sign-in. If omitted under
    /// --service-principal, raz fetches one from the GitHub Actions OIDC provider.
    #[arg(long, env = "AZURE_FEDERATED_TOKEN")]
    pub federated_token: Option<String>,

    /// Audience for the fetched GitHub Actions OIDC token.
    #[arg(long, default_value = "api://AzureADTokenExchange")]
    pub federated_token_audience: String,
}

pub async fn run(args: LoginArgs, _globals: &GlobalArgs) -> Result<()> {
    let http = new_http_client();
    let mut profile = Profile::load()?;
    let cloud = raz_core::cloud::resolve(profile.cloud.as_deref());
    if cloud.name != "AzureCloud" {
        println!("Cloud: {}", cloud.name);
    }

    let (tenant, token) = if args.identity {
        identity_login(&http, cloud, &args).await?
    } else if args.service_principal {
        service_principal_login(&http, cloud, &args).await?
    } else {
        device_code_login(&http, cloud, &profile, &args).await?
    };

    // Persist the token first so it can be reused for discovery and later commands.
    profile.tenant_id = Some(tenant);
    profile.token = Some(cache_from_response(&token));
    profile.save()?;

    // Enumerate the tenants/subscriptions the identity can reach. With a refresh token this is
    // cross-tenant; a service-principal token has none, so it degrades to its single tenant.
    let (tenants, subs) = discover_all(&http, cloud, &token).await?;

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

/// Interactive device-code flow. Resolves the tenant from the flag/env, else the last tenant
/// used, else the multi-tenant authority.
async fn device_code_login(
    http: &reqwest::Client,
    cloud: &raz_core::cloud::Cloud,
    profile: &Profile,
    args: &LoginArgs,
) -> Result<(String, TokenResponse)> {
    let tenant = args
        .tenant
        .clone()
        .or_else(|| profile.tenant_id.clone())
        .unwrap_or_else(|| "organizations".to_string());
    println!("Signing in to tenant: {tenant}");

    let token = device_code::run_flow(http, cloud.authority, &tenant, &cloud.arm_scope(), |dc| {
        println!("{}", dc.message);
        device_code::open_verification(dc);
    })
    .await?;
    Ok((tenant, token))
}

/// Managed-identity sign-in via IMDS. No refresh token; the ARM access token is cached and used
/// directly. `--client-id` selects a user-assigned identity.
async fn identity_login(
    http: &reqwest::Client,
    cloud: &raz_core::cloud::Cloud,
    args: &LoginArgs,
) -> Result<(String, TokenResponse)> {
    use raz_core::auth::managed_identity;
    println!("Signing in with managed identity…");
    let token = managed_identity::acquire(
        http,
        &cloud.arm_resource(),
        args.client_id.as_deref(),
        args.object_id.as_deref(),
        args.resource_id.as_deref(),
    )
    .await?;
    // Match az: take the tenant from the token's `tid` claim (override with --tenant).
    let tenant = args
        .tenant
        .clone()
        .or_else(|| managed_identity::tenant_from_token(&token.access_token))
        .unwrap_or_default();
    Ok((tenant, token))
}

/// Non-interactive service-principal sign-in: client secret if given, otherwise an OIDC
/// federated token (`--federated-token`, or fetched from the GitHub Actions OIDC provider).
async fn service_principal_login(
    http: &reqwest::Client,
    cloud: &raz_core::cloud::Cloud,
    args: &LoginArgs,
) -> Result<(String, TokenResponse)> {
    let client_id = args
        .client_id
        .clone()
        .ok_or_else(|| usage("--service-principal requires --client-id"))?;
    let tenant = args
        .tenant
        .clone()
        .ok_or_else(|| usage("--service-principal requires --tenant"))?;

    let token = if let Some(secret) = &args.client_secret {
        println!("Signing in as service principal {client_id} (client secret) to {tenant}");
        sp::acquire_client_secret(
            http,
            cloud.authority,
            cloud.arm,
            &tenant,
            &client_id,
            secret,
        )
        .await?
    } else {
        let assertion = match &args.federated_token {
            Some(t) => t.clone(),
            None => {
                println!("Fetching GitHub Actions OIDC token…");
                sp::fetch_github_oidc_token(http, &args.federated_token_audience).await?
            }
        };
        println!("Signing in as service principal {client_id} (federated token) to {tenant}");
        sp::acquire_federated(
            http,
            cloud.authority,
            cloud.arm,
            &tenant,
            &client_id,
            &assertion,
        )
        .await?
    };
    Ok((tenant, token))
}
