//! The [`Command`] trait — raz's analogue of an az `custom.py` handler.
//!
//! az registers a command table mapping `group + name` -> a Python function. We model a
//! handler as a type implementing [`Command`]; the CLI front-end builds the same logical
//! table via clap subcommands, and the TUI calls the same handlers directly. Each handler
//! returns a [`serde_json::Value`] plus an optional [`TableSpec`], which the front-end then
//! renders through [`crate::output::render`].

use async_trait::async_trait;
use serde_json::Value;

use crate::context::Context;
use crate::error::Result;
use crate::output::TableSpec;

/// What a command produces: a JSON payload and an optional table projection.
pub struct CommandOutput {
    pub value: Value,
    pub table: Option<TableSpec>,
}

impl CommandOutput {
    pub fn json(value: Value) -> Self {
        Self { value, table: None }
    }

    pub fn with_table(value: Value, table: TableSpec) -> Self {
        Self {
            value,
            table: Some(table),
        }
    }
}

/// An executable command. Implementors live in the `arm`/`auth` modules (data) and the
/// front-end command structs (wiring), keeping registration, args, and logic separated
/// the way az splits `commands.py` / `_params.py` / `custom.py`.
#[async_trait]
pub trait Command {
    async fn execute(&self, ctx: &Context) -> Result<CommandOutput>;
}
