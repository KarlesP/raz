//! `raz suggest ...` — offline tag and resource-name suggestions (no Azure calls).
//! Tags follow the Azure Landing Zones resource-tagging policies; names follow CAF guidance.

use clap::Subcommand;
use serde_json::{json, Value};

use raz_core::context::Context;
use raz_core::error::Result;
use raz_core::{suggest, GlobalArgs, OutputFormat};

use super::emit;

#[derive(Subcommand)]
pub enum SuggestCommand {
    /// Suggest a standard tag set (ALZ resource-tagging policy). Flags fill values.
    Tags {
        #[arg(long)]
        environment: Option<String>,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        workload: Option<String>,
        #[arg(long)]
        costcenter: Option<String>,
        #[arg(long)]
        sla: Option<String>,
        #[arg(long)]
        application: Option<String>,
        #[arg(long)]
        department: Option<String>,
    },
    /// Suggest a CAF-compliant resource name.
    Name {
        /// Resource type or CAF abbreviation (e.g. vm, storage, vnet, key-vault).
        #[arg(long = "type", short = 't')]
        kind: String,
        #[arg(long, short = 'w')]
        workload: String,
        #[arg(long, short = 'e')]
        env: String,
        #[arg(long, short = 'r')]
        region: String,
        #[arg(long, default_value = "001")]
        instance: String,
    },
}

pub async fn run(command: SuggestCommand, globals: GlobalArgs) -> Result<()> {
    match command {
        SuggestCommand::Tags {
            environment,
            owner,
            workload,
            costcenter,
            sla,
            application,
            department,
        } => {
            // Map each recommended tag to a provided value, the example otherwise.
            let provided = |key: &str| -> Option<String> {
                match key {
                    "environment" => environment.clone(),
                    "owner" => owner.clone(),
                    "workload" => workload.clone(),
                    "costcenter" => costcenter.clone(),
                    "sla" => sla.clone(),
                    "application" => application.clone(),
                    "department" => department.clone(),
                    _ => None,
                }
            };

            let rows: Vec<Value> = suggest::recommended_tags()
                .iter()
                .map(|t| {
                    json!({
                        "tag": t.key,
                        "value": provided(t.key).unwrap_or_else(|| format!("<{}>", t.key)),
                        "required": t.required,
                        "example": t.example,
                        "description": t.description,
                    })
                })
                .collect();

            // Ready-to-paste arg snippet (provided value, else example).
            let snippet = suggest::recommended_tags()
                .iter()
                .map(|t| {
                    let v = provided(t.key).unwrap_or_else(|| t.example.to_string());
                    format!("--tag {}=\"{}\"", t.key, v)
                })
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("Apply with: raz group create -n <rg> {snippet}");

            let ctx = Context::load(globals)?;
            emit(
                &ctx,
                Value::Array(rows),
                Some(&vec![
                    ("Tag", "tag"),
                    ("Value", "value"),
                    ("Required", "required"),
                    ("Description", "description"),
                ]),
            )
        }
        SuggestCommand::Name {
            kind,
            workload,
            env,
            region,
            instance,
        } => {
            let name = suggest::suggest_name(&kind, &workload, &env, &region, &instance);
            if matches!(globals.output, OutputFormat::Json) {
                let value = json!({
                    "name": name,
                    "abbreviation": suggest::abbreviation(&kind),
                    "pattern": suggest::NAME_PATTERN,
                });
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("{name}");
            }
            Ok(())
        }
    }
}
