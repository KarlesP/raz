//! `raz completion <shell>` — print a shell completion script (clap_complete). Mirrors az's
//! completion support; pipe/source the output per your shell.

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use raz_core::error::Result;

pub fn run(shell: Shell) -> Result<()> {
    let mut cmd = crate::Cli::command();
    generate(shell, &mut cmd, "raz", &mut std::io::stdout());
    Ok(())
}
