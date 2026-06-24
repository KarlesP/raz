//! `raz role ...` — RBAC role definitions and assignments. Mirrors `az role definition list`
//! and `az role assignment {list,create,delete}`. Assignees are principal **object ids**.

use clap::Subcommand;

use raz_core::arm::role;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum RoleCommand {
    /// Manage role assignments.
    Assignment {
        #[command(subcommand)]
        command: AssignmentCommand,
    },
    /// Inspect role definitions.
    Definition {
        #[command(subcommand)]
        command: DefinitionCommand,
    },
}

#[derive(Subcommand)]
pub enum AssignmentCommand {
    /// List role assignments at a scope (subscription, or a resource group with -g).
    List {
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
        /// Filter by principal object id.
        #[arg(long)]
        assignee: Option<String>,
    },
    /// Assign a role to a principal.
    Create {
        /// Role name (e.g. Contributor) or role-definition GUID.
        #[arg(long)]
        role: String,
        /// Principal object id (user/group/service-principal).
        #[arg(long)]
        assignee: String,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
        /// Principal type hint (User | Group | ServicePrincipal).
        #[arg(long)]
        assignee_principal_type: Option<String>,
    },
    /// Remove a role assignment from a principal.
    Delete {
        #[arg(long)]
        role: String,
        #[arg(long)]
        assignee: String,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DefinitionCommand {
    /// List role definitions (optionally filter by role name).
    List {
        #[arg(long, short = 'n')]
        name: Option<String>,
        #[arg(long, short = 'g')]
        resource_group: Option<String>,
    },
}

pub async fn run(command: RoleCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        RoleCommand::Definition {
            command:
                DefinitionCommand::List {
                    name,
                    resource_group,
                },
        } => {
            let (ctx, client, sub) = arm_context(globals).await?;
            let scope = role::scope(&sub, resource_group.as_deref());
            let value = role::list_definitions(&client, &scope, name.as_deref()).await?;
            emit(&ctx, value, Some(&role::definition_table()))
        }
        RoleCommand::Assignment { command } => match command {
            AssignmentCommand::List {
                resource_group,
                assignee,
            } => {
                let (ctx, client, sub) = arm_context(globals).await?;
                let scope = role::scope(&sub, resource_group.as_deref());
                let value = role::list_assignments(&client, &scope, assignee.as_deref()).await?;
                emit(&ctx, value, Some(&role::assignment_table()))
            }
            AssignmentCommand::Create {
                role: role_name,
                assignee,
                resource_group,
                assignee_principal_type,
            } => {
                let (ctx, client, sub) = arm_context(globals).await?;
                let scope = role::scope(&sub, resource_group.as_deref());
                let value = role::create_assignment(
                    &client,
                    &sub,
                    &scope,
                    &role_name,
                    &assignee,
                    assignee_principal_type.as_deref(),
                )
                .await?;
                emit(&ctx, value, Some(&role::assignment_table()))
            }
            AssignmentCommand::Delete {
                role: role_name,
                assignee,
                resource_group,
            } => {
                let (_ctx, client, sub) = arm_context(globals).await?;
                let scope = role::scope(&sub, resource_group.as_deref());
                role::delete_assignment(&client, &sub, &scope, &role_name, &assignee).await?;
                println!("Removed role '{role_name}' from principal {assignee}.");
                Ok(())
            }
        },
    }
}
