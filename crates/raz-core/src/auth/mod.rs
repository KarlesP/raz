//! Authentication: the OAuth 2.0 device-code login against Microsoft Entra, plus the helper
//! that turns its token response into a cache entry. Backs `raz login` / `raz logout`.

pub mod device_code;

use crate::config::CachedToken;
use device_code::TokenResponse;

/// Wall-clock time as Unix seconds. Centralized so token-expiry logic has a single source.
pub fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Turn a freshly-acquired token into a cache entry, converting the relative `expires_in`
/// into an absolute Unix expiry so later commands can check staleness without the issue time.
pub fn cache_from_response(resp: &TokenResponse) -> CachedToken {
    CachedToken {
        access_token: resp.access_token.clone(),
        refresh_token: resp.refresh_token.clone(),
        expires_on: now_unix() + resp.expires_in,
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
        let now = now_unix();
        assert!(cached.expires_on >= now + 3500 && cached.expires_on <= now + 3700);
    }
}
