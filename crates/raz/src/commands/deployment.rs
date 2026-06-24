//! `raz deployment group ...` — deploy an ARM JSON / Bicep template, or preview with what-if.

use std::process::Command;

use clap::Subcommand;
use serde_json::Value;

use raz_core::arm::deployment;
use raz_core::error::{usage, Result};
use raz_core::GlobalArgs;

use super::{arm_context, emit};

#[derive(Subcommand)]
pub enum DeploymentCommand {
    /// Resource-group scoped deployments.
    Group {
        #[command(subcommand)]
        command: GroupDeploymentCommand,
    },
}

#[derive(Subcommand)]
pub enum GroupDeploymentCommand {
    /// Deploy a template to a resource group and wait for completion.
    Create {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n', default_value = "raz-deploy")]
        name: String,
        /// Path to an ARM JSON template (or a .bicep file, compiled via the bicep CLI).
        #[arg(long)]
        template_file: String,
        /// Path to a parameters file (ARM params-file or a plain key→value object).
        #[arg(long)]
        parameters: Option<String>,
        /// Incremental or Complete.
        #[arg(long, default_value = "Incremental")]
        mode: String,
    },
    /// Preview the changes a template would make, without applying.
    WhatIf {
        #[arg(long, short = 'g')]
        resource_group: String,
        #[arg(long, short = 'n', default_value = "raz-whatif")]
        name: String,
        #[arg(long)]
        template_file: String,
        #[arg(long)]
        parameters: Option<String>,
        #[arg(long, default_value = "Incremental")]
        mode: String,
    },
}

pub async fn run(command: DeploymentCommand, globals: GlobalArgs) -> Result<()> {
    let DeploymentCommand::Group { command } = command;
    match command {
        GroupDeploymentCommand::Create {
            resource_group,
            name,
            template_file,
            parameters,
            mode,
        } => {
            let template = load_template(&template_file)?;
            let params = load_parameters(parameters.as_deref())?;
            let (ctx, client, sub) = arm_context(globals).await?;
            eprintln!("Deploying '{name}' to {resource_group}…");
            let value = deployment::create(
                &client,
                &sub,
                &resource_group,
                &name,
                template,
                params,
                &mode,
            )
            .await?;
            emit(&ctx, value, Some(&deployment::create_table_spec()))
        }
        GroupDeploymentCommand::WhatIf {
            resource_group,
            name,
            template_file,
            parameters,
            mode,
        } => {
            let template = load_template(&template_file)?;
            let params = load_parameters(parameters.as_deref())?;
            let (ctx, client, sub) = arm_context(globals).await?;
            let value = deployment::what_if(
                &client,
                &sub,
                &resource_group,
                &name,
                template,
                params,
                &mode,
            )
            .await?;
            emit(&ctx, value, Some(&deployment::whatif_table_spec()))
        }
    }
}

/// Load a template: compile `.bicep` via the bicep CLI, otherwise parse the JSON file.
// ponytail: shell out to the native bicep CLI, no Rust compiler reimplementation.
fn load_template(path: &str) -> Result<Value> {
    let text = if path.ends_with(".bicep") {
        let out = Command::new("bicep")
            .args(["build", "--file", path, "--stdout"])
            .output()
            .map_err(|_| usage("bicep CLI not found; install it or pass a JSON template"))?;
        if !out.status.success() {
            return Err(usage(format!(
                "bicep build failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        String::from_utf8_lossy(&out.stdout).into_owned()
    } else {
        std::fs::read_to_string(path)?
    };
    serde_json::from_str(&text).map_err(|e| usage(format!("template is not valid JSON: {e}")))
}

/// Load parameters: none → `{}`; an ARM params-file → its `parameters` object; else as-is.
fn load_parameters(path: Option<&str>) -> Result<Value> {
    let Some(path) = path else {
        return Ok(serde_json::json!({}));
    };
    let text = std::fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| usage(format!("parameters is not valid JSON: {e}")))?;
    Ok(extract_params(value))
}

/// An ARM params-file wraps values under `parameters`; a plain object is used as-is.
fn extract_params(value: Value) -> Value {
    value.get("parameters").cloned().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::extract_params;
    use serde_json::json;

    #[test]
    fn extract_params_handles_both_shapes() {
        // ARM params-file: the inner `parameters` object is used.
        assert_eq!(
            extract_params(json!({"$schema":"x","parameters":{"loc":{"value":"weu"}}})),
            json!({"loc":{"value":"weu"}})
        );
        // Plain object: used as-is.
        let plain = json!({"loc":{"value":"weu"}});
        assert_eq!(extract_params(plain.clone()), plain);
    }
}
