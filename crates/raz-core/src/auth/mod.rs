//! Authentication: the OAuth 2.0 device-code flow against Microsoft Entra, plus a
//! cached-token credential. This is raz's `raz login` / `raz logout` backend, the
//! analogue of az's `profile/custom.py`.
//!
//! In a production port the credential here would be an `azure_identity` implementation of
//! `azure_core::credentials::TokenCredential`; the [`credential::TokenSource`] trait is
//! shaped to make that substitution mechanical.

pub mod credential;
pub mod device_code;

/// Current wall-clock time as Unix seconds. Centralized so tests can reason about expiry.
pub fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
