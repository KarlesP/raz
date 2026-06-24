//! Error type and exit-code mapping.
//!
//! Exit codes mirror the Azure CLI contract (see azure-cli `README.md`):
//! 0 success, 1 generic error, 2 parser/usage error, 3 missing ARM resource.

use std::fmt;

pub type Result<T> = std::result::Result<T, RazError>;

#[derive(Debug, thiserror::Error)]
pub enum RazError {
    /// User is not logged in or the cached token is unusable.
    #[error("not logged in: run `raz login` first")]
    NotLoggedIn,

    /// Invalid arguments / usage. Maps to exit code 2.
    #[error("invalid usage: {0}")]
    Usage(String),

    /// A requested ARM resource was not found. Maps to exit code 3.
    #[error("resource not found: {0}")]
    NotFound(String),

    /// Authentication failed (device-code flow, token exchange, etc.).
    #[error("authentication failed: {0}")]
    Auth(String),

    /// Any transport / HTTP error talking to Azure.
    #[error("request failed: {0}")]
    Http(String),

    /// I/O or config persistence error.
    #[error("io error: {0}")]
    Io(String),

    /// Anything else.
    #[error("{0}")]
    Other(String),
}

impl RazError {
    /// Process exit code following the az convention.
    pub fn exit_code(&self) -> i32 {
        match self {
            RazError::Usage(_) => 2,
            RazError::NotFound(_) => 3,
            _ => 1,
        }
    }
}

impl From<std::io::Error> for RazError {
    fn from(e: std::io::Error) -> Self {
        RazError::Io(e.to_string())
    }
}

impl From<reqwest::Error> for RazError {
    fn from(e: reqwest::Error) -> Self {
        RazError::Http(e.to_string())
    }
}

impl From<serde_json::Error> for RazError {
    fn from(e: serde_json::Error) -> Self {
        RazError::Other(format!("json: {e}"))
    }
}

/// Helper so commands can produce a [`RazError::Usage`] tersely.
pub fn usage(msg: impl fmt::Display) -> RazError {
    RazError::Usage(msg.to_string())
}

/// Map an HTTP status from an Azure service (`service` = "ARM"/"Graph") onto a [`RazError`],
/// preserving az-compatible exit codes. Shared by the ARM and Graph clients.
pub(crate) fn map_http_status(service: &str, status: u16, path: &str, body: String) -> RazError {
    match status {
        401 => RazError::NotLoggedIn,
        403 => RazError::Auth(format!("forbidden: {body}")),
        404 => RazError::NotFound(path.to_string()),
        _ => RazError::Http(format!("{service} {status}: {body}")),
    }
}
