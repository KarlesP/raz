//! `raz aks ...` — Azure Kubernetes Service: list/show clusters and fetch the kubeconfig.

use base64::{engine::general_purpose::STANDARD, Engine};
use clap::Subcommand;

use raz_core::arm::aks;
use raz_core::error::{RazError, Result};
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum AksCommand {
    /// List AKS clusters in the subscription.
    List,
    /// Show an AKS cluster.
    Show {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Fetch the cluster-user kubeconfig (prints it, or writes it with --file).
    GetCredentials {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n')]
        name: String,
        /// Write the kubeconfig to this path instead of stdout.
        #[arg(long)]
        file: Option<String>,
    },
}

pub async fn run(command: AksCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        AksCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = aks::list(&client, &sub).await?;
            emit(&ctx, value, Some(&aks::table_spec()))
        }
        AksCommand::Show {
            resource_group,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = aks::show(&client, &sub, &resource_group, &name).await?;
            emit(&ctx, value, Some(&aks::table_spec()))
        }
        AksCommand::GetCredentials {
            resource_group,
            name,
            file,
        } => {
            let (_ctx, client, sub) = arm_context(globals).await?;
            let encoded = aks::get_credentials(&client, &sub, &resource_group, &name).await?;
            let bytes = STANDARD
                .decode(encoded.as_bytes())
                .map_err(|e| RazError::Other(format!("kubeconfig is not valid base64: {e}")))?;
            let kubeconfig = String::from_utf8(bytes)
                .map_err(|e| RazError::Other(format!("kubeconfig is not valid UTF-8: {e}")))?;
            match file {
                Some(path) => {
                    std::fs::write(&path, kubeconfig)?;
                    println!("Wrote kubeconfig to {path}.");
                }
                None => print!("{kubeconfig}"),
            }
            Ok(())
        }
    }
}
