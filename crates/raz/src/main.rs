//! `raz` — the minimal CLI front-end. clap models the same command tree az exposes
//! (`login`, `logout`, `vnet ...`, `vm ...`); each leaf dispatches into raz-core and the
//! result is rendered through the shared output formatter. The process exit code follows
//! az's contract via [`raz_core::RazError::exit_code`].

mod commands;
mod schema;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

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

    /// Output format. Defaults to the configured default (`raz configure`), else json.
    #[arg(long, short = 'o', global = true)]
    output: Option<String>,

    /// JMESPath query applied to the JSON result (like az `--query`).
    #[arg(long, global = true)]
    query: Option<String>,

    /// Don't wait for long-running operations to finish (az `--no-wait`).
    #[arg(long, global = true)]
    no_wait: bool,
}

impl GlobalOpts {
    fn to_core(&self, defaults: &raz_core::config::Defaults) -> Result<GlobalArgs, RazError> {
        // Output precedence: explicit -o, then the configured default, then json.
        let output = match self.output.clone().or_else(|| defaults.output.clone()) {
            Some(s) => s.parse::<OutputFormat>()?,
            None => OutputFormat::Json,
        };
        Ok(GlobalArgs {
            subscription: self.subscription.clone(),
            output,
            query: self.query.clone(),
            no_wait: self.no_wait,
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
    /// Suggest standard tags and CAF-compliant resource names (offline).
    Suggest {
        #[command(subcommand)]
        command: commands::suggest::SuggestCommand,
    },
    /// Scan governance / policy assignments + compliance.
    Policy {
        #[command(subcommand)]
        command: commands::policy::PolicyCommand,
    },
    /// Manage resource tags.
    Tag {
        #[command(subcommand)]
        command: commands::tag::TagCommand,
    },
    /// Manage resource locks (CanNotDelete / ReadOnly).
    Lock {
        #[command(subcommand)]
        command: commands::lock::LockCommand,
    },
    /// Create a subscription (alias API) / check its state.
    Subscription {
        #[command(subcommand)]
        command: commands::subscription::SubscriptionCommand,
    },
    /// Manage cost-management budgets.
    Budget {
        #[command(subcommand)]
        command: commands::budget::BudgetCommand,
    },
    /// Deploy ARM/Bicep templates (create / what-if).
    Deployment {
        #[command(subcommand)]
        command: commands::deployment::DeploymentCommand,
    },
    /// Manage storage accounts and blob containers.
    Storage {
        #[command(subcommand)]
        command: commands::storage::StorageCommand,
    },
    /// Manage key vaults and secrets.
    Keyvault {
        #[command(subcommand)]
        command: commands::keyvault::KeyvaultCommand,
    },
    /// Network resources — NSGs, public IPs, NICs.
    Network {
        #[command(subcommand)]
        command: commands::network::NetworkCommand,
    },
    /// Manage AKS clusters and fetch kubeconfig.
    Aks {
        #[command(subcommand)]
        command: commands::aks::AksCommand,
    },
    /// Azure Monitor — metrics and activity log.
    Monitor {
        #[command(subcommand)]
        command: commands::monitor::MonitorCommand,
    },
    /// Manage App Service web apps.
    Webapp {
        #[command(subcommand)]
        command: commands::webapp::WebappCommand,
    },
    /// Manage App Service plans.
    Appservice {
        #[command(subcommand)]
        command: commands::appservice::AppserviceCommand,
    },
    /// Print a shell completion script (bash, zsh, fish, powershell, elvish).
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// View or set persisted defaults (location / output) in `~/.raz`.
    Configure(commands::configure::ConfigureArgs),
    /// Wait until a resource reaches a state (created / deleted / exists / custom).
    Wait(commands::wait::WaitArgs),
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
    let defaults = raz_core::config::Profile::load()?.defaults;
    let globals = cli.globals.to_core(&defaults)?;
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
        TopCommand::Suggest { command } => commands::suggest::run(command, globals).await,
        TopCommand::Policy { command } => commands::policy::run(command, globals).await,
        TopCommand::Tag { command } => commands::tag::run(command, globals).await,
        TopCommand::Lock { command } => commands::lock::run(command, globals).await,
        TopCommand::Subscription { command } => commands::subscription::run(command, globals).await,
        TopCommand::Budget { command } => commands::budget::run(command, globals).await,
        TopCommand::Deployment { command } => commands::deployment::run(command, globals).await,
        TopCommand::Storage { command } => commands::storage::run(command, globals).await,
        TopCommand::Keyvault { command } => commands::keyvault::run(command, globals).await,
        TopCommand::Network { command } => commands::network::run(command, globals).await,
        TopCommand::Aks { command } => commands::aks::run(command, globals).await,
        TopCommand::Monitor { command } => commands::monitor::run(command, globals).await,
        TopCommand::Webapp { command } => commands::webapp::run(command, globals).await,
        TopCommand::Appservice { command } => commands::appservice::run(command, globals).await,
        TopCommand::Completion { shell } => commands::completion::run(shell),
        TopCommand::Configure(args) => commands::configure::run(args, globals),
        TopCommand::Wait(args) => commands::wait::run(args, globals).await,
    }
}
