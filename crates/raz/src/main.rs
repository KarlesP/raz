//! `raz` — the minimal CLI front-end. clap models the same command tree az exposes
//! (`login`, `logout`, `vnet ...`, `vm ...`); each leaf dispatches into raz-core and the
//! result is rendered through the shared output formatter. The process exit code follows
//! az's contract via [`raz_core::RazError::exit_code`].

mod commands;
mod schema;

use clap::{Args, Parser, Subcommand};

use raz_core::error::RazError;
use raz_core::{GlobalArgs, OutputFormat};

#[derive(Parser)]
#[command(
    name = "raz",
    version,
    about = "A Rust port of a slice of the Azure CLI"
)]
struct Cli {
    #[command(flatten)]
    globals: GlobalOpts,

    #[command(subcommand)]
    command: TopCommand,
}

/// Global options available on every subcommand (az: `--subscription`, `--output`, `--query`).
#[derive(Args)]
struct GlobalOpts {
    /// Name or ID of subscription to target.
    #[arg(long, short = 's', global = true)]
    subscription: Option<String>,

    /// Output format.
    #[arg(long, short = 'o', global = true, default_value = "json")]
    output: String,

    /// Minimal dotted-path projection of the JSON result (subset of JMESPath).
    #[arg(long, global = true)]
    query: Option<String>,
}

impl GlobalOpts {
    fn to_core(&self) -> Result<GlobalArgs, RazError> {
        Ok(GlobalArgs {
            subscription: self.subscription.clone(),
            output: self.output.parse::<OutputFormat>()?,
            query: self.query.clone(),
        })
    }
}

#[derive(Subcommand)]
enum TopCommand {
    /// Log in to Azure via the device-code flow.
    Login(commands::login::LoginArgs),
    /// Log out and clear the cached profile.
    Logout,
    /// Manage and switch the active subscription / view tenants.
    Account {
        #[command(subcommand)]
        command: commands::account::AccountCommand,
    },
    /// Microsoft Entra directory operations (app federated credentials).
    Ad {
        #[command(subcommand)]
        command: commands::ad::AdCommand,
    },
    /// Manage resource groups.
    Group {
        #[command(subcommand)]
        command: commands::group::GroupCommand,
    },
    /// Manage RBAC role definitions and assignments.
    Role {
        #[command(subcommand)]
        command: commands::role::RoleCommand,
    },
    /// Generic CRUD over any resource type/id.
    Resource {
        #[command(subcommand)]
        command: commands::resource::ResourceCommand,
    },
    /// Manage virtual networks.
    Vnet {
        #[command(subcommand)]
        command: commands::vnet::VnetCommand,
    },
    /// Manage virtual machines.
    Vm {
        #[command(subcommand)]
        command: commands::vm::VmCommand,
    },
    /// Make an arbitrary authenticated ARM/Graph REST call.
    Rest(commands::rest::RestArgs),
}

#[tokio::main]
async fn main() {
    // Hidden introspection command: dump the clap command tree as JSON for raz-tui's
    // autocomplete. Intercepted before clap parsing so it never appears in help/usage.
    if std::env::args().nth(1).as_deref() == Some("__schema") {
        schema::print::<Cli>();
        return;
    }

    let cli = Cli::parse();
    let code = match run(cli).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("raz: {e}");
            e.exit_code()
        }
    };
    std::process::exit(code);
}

async fn run(cli: Cli) -> Result<(), RazError> {
    let globals = cli.globals.to_core()?;
    match cli.command {
        TopCommand::Login(args) => commands::login::run(args, &globals).await,
        TopCommand::Logout => commands::logout::run().await,
        TopCommand::Account { command } => commands::account::run(command, globals).await,
        TopCommand::Ad { command } => commands::ad::run(command, globals).await,
        TopCommand::Group { command } => commands::group::run(command, globals).await,
        TopCommand::Role { command } => commands::role::run(command, globals).await,
        TopCommand::Resource { command } => commands::resource::run(command, globals).await,
        TopCommand::Vnet { command } => commands::vnet::run(command, globals).await,
        TopCommand::Vm { command } => commands::vm::run(command, globals).await,
        TopCommand::Rest(args) => commands::rest::run(args, globals).await,
    }
}
