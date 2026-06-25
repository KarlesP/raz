//! `raz extension ...` — placeholder for the planned extension system (see FEATURES.md #26).
//! Not available yet; the commands exist so the surface is reserved and discoverable.

use clap::Subcommand;

use raz_core::error::Result;

#[derive(Subcommand)]
pub enum ExtensionCommand {
    /// List installed extensions.
    List,
    /// Install an extension.
    Add {
        #[arg(long, short = 'n')]
        name: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
    /// Remove an extension.
    Remove {
        #[arg(long, short = 'n')]
        name: String,
    },
}

pub fn run(_command: ExtensionCommand) -> Result<()> {
    eprintln!("raz: extensions are not available at the moment (planned — see the roadmap).");
    Ok(())
}
