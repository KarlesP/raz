//! `raz vm ...` — virtual machine commands. Mirrors az's `vm` group.

use clap::Subcommand;

use raz_core::arm::vm;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum VmCommand {
    /// List virtual machines in the subscription.
    List,
    /// Show a single virtual machine.
    Show {
        /// Resource group name.
        #[arg(long, short = 'g')]
        resource_group: String,
        /// Virtual machine name.
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Create a virtual machine (stubbed in this skeleton).
    Create {
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Delete a virtual machine (stubbed in this skeleton).
    Delete {
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Start a virtual machine (stubbed in this skeleton).
    Start {
        #[arg(long, short = 'n')]
        name: String,
    },
    /// Stop (power off) a virtual machine (stubbed in this skeleton).
    Stop {
        #[arg(long, short = 'n')]
        name: String,
    },
}

pub async fn run(command: VmCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        VmCommand::List => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = vm::list(&client, &sub).await?;
            emit(&ctx, value, Some(&vm::table_spec()))
        }
        VmCommand::Show {
            resource_group,
            name,
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = vm::show(&client, &sub, &resource_group, &name).await?;
            emit(&ctx, value, Some(&vm::table_spec()))
        }
        VmCommand::Create { name } => vm::create(&name).await.map(|_| ()),
        VmCommand::Delete { name } => vm::delete(&name).await.map(|_| ()),
        VmCommand::Start { name } => vm::start(&name).await.map(|_| ()),
        VmCommand::Stop { name } => vm::stop(&name).await.map(|_| ()),
    }
}
