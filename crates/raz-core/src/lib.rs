//! raz-core — the reusable engine behind the `raz` CLI and the `raz-tui` dashboard.
//!
//! The module layout mirrors how the Azure CLI organizes a command module, but in Rust:
//! - [`command`] — the `Command` trait (az `custom.py` handler analogue).
//! - [`context`] — the shared `Context`/`GlobalArgs` (az `cli_ctx` + global args).
//! - [`config`] — the `~/.raz` profile + token cache (az `~/.azure`).
//! - [`output`] — json/table/tsv rendering (az `--output` + table transformers).
//! - [`auth`] — device-code login flow + cached-token credential.
//! - [`arm`] — Azure Resource Manager REST client and the vnet/vm operations.
//! - [`error`] — `RazError` and the az-compatible exit-code mapping.

pub mod arm;
pub mod auth;
pub mod command;
pub mod config;
pub mod context;
pub mod error;
pub mod output;

pub use command::{Command, CommandOutput};
pub use context::{Context, GlobalArgs};
pub use error::{RazError, Result};
pub use output::{OutputFormat, TableSpec};
