//! raz-core — the engine shared by the `raz` CLI and the `raz-tui` dashboard.
//!
//! Modules mirror how the Azure CLI splits a command module: [`context`] is the shared
//! request context, [`config`] persists the `~/.raz` profile, [`output`] renders
//! json/table/tsv, [`auth`] runs the device-code login, [`arm`] is the ARM REST client plus
//! the group/vnet/vm operations, and [`error`] maps failures to az-compatible exit codes.

pub mod arm;
pub mod auth;
pub mod cloud;
pub mod config;
pub mod context;
pub mod error;
pub mod graph;
mod odata;
pub mod output;
pub mod suggest;

pub use context::{Context, GlobalArgs};
pub use error::{RazError, Result};
pub use output::{OutputFormat, TableSpec};
