//! A token source abstraction and the conversion from a device-code [`TokenResponse`]
//! into a persisted [`CachedToken`].
//!
//! [`TokenSource`] is the seam where an `azure_identity` credential (implementing
//! `azure_core::credentials::TokenCredential`) would slot in for production use.

use async_trait::async_trait;

use super::device_code::TokenResponse;
use super::now_unix;
use crate::config::CachedToken;
use crate::error::Result;

/// Anything that can yield a currently-valid bearer token for ARM.
#[async_trait]
pub trait TokenSource: Send + Sync {
    async fn get_token(&self) -> Result<String>;
}

/// Convert a freshly-acquired device-code token into a cache entry with an absolute expiry.
pub fn cache_from_response(resp: &TokenResponse) -> CachedToken {
    CachedToken {
        access_token: resp.access_token.clone(),
        refresh_token: resp.refresh_token.clone(),
        expires_on: now_unix() + resp.expires_in,
    }
}

/// A trivial [`TokenSource`] over an already-cached token (no refresh). Used by the ARM
/// client; refresh-on-expiry is intentionally left to a future `azure_identity`-backed
/// implementation.
pub struct CachedTokenSource {
    token: CachedToken,
}

impl CachedTokenSource {
    pub fn new(token: CachedToken) -> Self {
        Self { token }
    }
}

#[async_trait]
impl TokenSource for CachedTokenSource {
    async fn get_token(&self) -> Result<String> {
        if self.token.is_expired(now_unix(), 60) {
            return Err(crate::error::RazError::NotLoggedIn);
        }
        Ok(self.token.access_token.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_from_response_sets_absolute_expiry() {
        let resp = TokenResponse {
            access_token: "tok".into(),
            refresh_token: None,
            expires_in: 3600,
        };
        let cached = cache_from_response(&resp);
        assert_eq!(cached.access_token, "tok");
        // Expiry should be ~now+3600; allow a generous window for slow CI.
        let now = now_unix();
        assert!(cached.expires_on >= now + 3500 && cached.expires_on <= now + 3700);
    }
}
