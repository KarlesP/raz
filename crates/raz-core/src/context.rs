//! The execution [`Context`] threaded through every command: the HTTP client, the loaded
//! profile, and the global options. The az analogue of `cli_ctx` plus global args.

use crate::config::Profile;
use crate::error::{RazError, Result};
use crate::output::OutputFormat;

/// Command-independent options that apply to any subcommand (`--subscription`, `--output`,
/// `--query`). Parsed by the front-ends and handed to the context.
#[derive(Debug, Clone, Default)]
pub struct GlobalArgs {
    pub subscription: Option<String>,
    pub output: OutputFormat,
    /// Dotted-path projection of JSON output (a small subset of az's JMESPath `--query`).
    pub query: Option<String>,
}

/// Shared HTTP client constructor, so login and the ARM client use identical settings.
pub fn new_http_client() -> reqwest::Client {
    reqwest::Client::new()
}

pub struct Context {
    pub http: reqwest::Client,
    pub profile: Profile,
    pub globals: GlobalArgs,
}

impl Context {
    /// Build a context, loading the persisted profile from `~/.raz`.
    pub fn load(globals: GlobalArgs) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::new(),
            profile: Profile::load()?,
            globals,
        })
    }

    /// A non-expired cached bearer token, else [`RazError::NotLoggedIn`]. Used only as the
    /// fallback when there is no refresh token to mint a tenant-scoped token from.
    pub fn access_token(&self) -> Result<String> {
        let now = crate::auth::now_unix();
        match &self.profile.token {
            Some(tok) if !tok.is_expired(now, 60) => Ok(tok.access_token.clone()),
            _ => Err(RazError::NotLoggedIn),
        }
    }

    /// The subscription this invocation targets: the `--subscription` match (by id or name)
    /// else the profile default.
    pub fn active_subscription(&self) -> Option<&crate::config::Subscription> {
        match &self.globals.subscription {
            Some(want) => self
                .profile
                .subscriptions
                .iter()
                .find(|s| &s.id == want || &s.name == want),
            None => self.profile.default_subscription(),
        }
    }

    /// Resolve the target subscription id and an ARM token **for that subscription's tenant**.
    ///
    /// Because ARM tokens are tenant-scoped and a logged-in identity may span tenants, we mint
    /// a fresh token for the subscription's tenant from the stored refresh token. If no
    /// refresh token is available we fall back to the cached access token.
    pub async fn subscription_and_token(&self) -> Result<(String, String)> {
        let (sub_id, tenant) = match self.active_subscription() {
            Some(sub) => (sub.id.clone(), sub.tenant_id.clone()),
            None => {
                // A raw `--subscription <id>` not in the cached list: use the profile tenant.
                let sub = self
                    .globals
                    .subscription
                    .clone()
                    .ok_or(RazError::NotLoggedIn)?;
                let tenant = self.profile.tenant_id.clone().unwrap_or_default();
                (sub, tenant)
            }
        };

        let token = self.token_for_tenant(&tenant).await?;
        Ok((sub_id, token))
    }

    /// Mint an access token for `tenant` from the stored refresh token, falling back to the
    /// cached (home-tenant) access token when no refresh token is present.
    async fn token_for_tenant(&self, tenant: &str) -> Result<String> {
        let cached = self.profile.token.as_ref().ok_or(RazError::NotLoggedIn)?;
        if !tenant.is_empty() {
            if let Some(refresh) = &cached.refresh_token {
                let tok =
                    crate::auth::device_code::exchange_refresh_token(&self.http, tenant, refresh)
                        .await?;
                return Ok(tok.access_token);
            }
        }
        self.access_token()
    }
}
