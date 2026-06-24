//! `raz tag ...` — add/list/delete tags on any resource, resource group, or subscription.

use clap::Subcommand;

use raz_core::arm::tag;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit, parse_tags};

#[derive(Subcommand)]
pub enum TagCommand {
    /// List the tags on a resource (by full ARM id).
    List {
        #[arg(long)]
        resource: String,
    },
    /// Add or overwrite tags on a resource.
    Add {
        #[arg(long)]
        resource: String,
        /// Tag in `key=value` form (repeatable).
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Remove named tags from a resource.
    Delete {
        #[arg(long)]
        resource: String,
        /// Tag key to remove (repeatable).
        #[arg(long = "tag-name")]
        tag_names: Vec<String>,
    },
}

pub async fn run(command: TagCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        TagCommand::List { resource } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let value = tag::list(&client, &resource).await?;
            emit(&ctx, value, Some(&tag::table_spec()))
        }
        TagCommand::Add { resource, tags } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let pairs = parse_tags(&tags)?;
            let value = tag::add(&client, &resource, &pairs).await?;
            emit(&ctx, value, Some(&tag::table_spec()))
        }
        TagCommand::Delete {
            resource,
            tag_names,
        } => {
            let (ctx, client, _sub) = arm_context(globals).await?;
            let value = tag::delete(&client, &resource, &tag_names).await?;
            emit(&ctx, value, Some(&tag::table_spec()))
        }
    }
}
