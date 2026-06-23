//! Authentication: the OAuth 2.0 device-code login against Microsoft Entra, plus the helper
//! that turns its token response into a cache entry. Backs `raz login` / `raz logout`.

pub mod credential;
pub mod device_code;

/// Wall-clock time as Unix seconds. Centralized so token-expiry logic has a single source.
pub fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
