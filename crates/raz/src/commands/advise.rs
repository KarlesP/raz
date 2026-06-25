//! `raz advise` — analyze the active subscription's architecture (CAF naming, ALZ tags, governance)
//! and surface a profile + guardrail warnings, so you don't ship out of scope. Phase 1 is
//! model-free; `--summary` (a GGUF/endpoint advisor) is planned.

use clap::Args;

use raz_core::arm::{group, policy, resource};
use raz_core::error::Result;
use raz_core::{advisor, llm, GlobalArgs};

use super::arm_context;

#[derive(Args)]
pub struct AdviseArgs {
    /// Limit the scan to a resource type (e.g. Microsoft.Compute/virtualMachines).
    #[arg(long)]
    service: Option<String>,
    /// Emit the structured profile as JSON instead of the readable report.
    #[arg(long)]
    json: bool,
    /// Generate a natural-language review via an LLM (endpoint by default, or --gguf for embedded).
    #[arg(long)]
    summary: bool,
    /// Embedded GGUF model file — loaded in-process via candle (build with `--features local-llm`).
    #[arg(long)]
    gguf: Option<String>,
    /// tokenizer.json for the GGUF (defaults to one beside the .gguf).
    #[arg(long)]
    tokenizer: Option<String>,
    /// LLM endpoint (OpenAI-compatible), e.g. a local llama.cpp / mistral.rs / Ollama server.
    #[arg(long, default_value = "http://localhost:8080/v1")]
    endpoint: String,
    /// Model name the endpoint expects (local servers usually accept anything).
    #[arg(long, default_value = "local")]
    model: String,
    /// Optional API key for the endpoint.
    #[arg(long)]
    api_key: Option<String>,
}

const SYSTEM: &str = "You are a senior Azure architecture reviewer. Be concise and concrete; \
    explain what the architecture follows, then list needs, worries, and opportunities, and flag \
    anything likely out of scope.";

pub async fn run(args: AdviseArgs, globals: GlobalArgs) -> Result<()> {
    let (ctx, client, sub) = arm_context(globals).await?;
    let resources = resource::list(&client, &sub, None, args.service.as_deref()).await?;
    let groups = group::list(&client, &sub).await?;
    let governance = policy::scan(&client, &sub, None).await?;

    let profile = advisor::analyze(&resources, &groups, &governance, &sub);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
        return Ok(());
    }

    print_report(&profile);

    if args.summary {
        let prompt = advisor::build_prompt(&profile);
        let review = if let Some(gguf) = &args.gguf {
            eprintln!("\nGenerating review with embedded GGUF {gguf}…");
            run_local(gguf, args.tokenizer.as_deref(), &prompt)?
        } else {
            eprintln!(
                "\nGenerating review via {} (model: {})…",
                args.endpoint, args.model
            );
            llm::complete(
                &ctx.http,
                &args.endpoint,
                &args.model,
                args.api_key.as_deref(),
                SYSTEM,
                &prompt,
            )
            .await?
        };
        println!("\nAI Review:\n{review}");
    }
    Ok(())
}

/// Run the embedded candle GGUF backend (only compiled with `--features local-llm`).
#[cfg(feature = "local-llm")]
fn run_local(gguf: &str, tokenizer: Option<&str>, prompt: &str) -> Result<String> {
    use raz_core::error::usage;
    use std::path::{Path, PathBuf};
    let gguf_path = Path::new(gguf);
    let tok = tokenizer
        .map(PathBuf::from)
        .unwrap_or_else(|| gguf_path.with_file_name("tokenizer.json"));
    if !tok.exists() {
        return Err(usage(format!(
            "tokenizer.json not found at {} — pass --tokenizer or place it beside the .gguf",
            tok.display()
        )));
    }
    raz_core::llm::local::generate(gguf_path, &tok, SYSTEM, prompt, 512)
}

#[cfg(not(feature = "local-llm"))]
fn run_local(_gguf: &str, _tokenizer: Option<&str>, _prompt: &str) -> Result<String> {
    Err(raz_core::error::usage(
        "this raz was built without GGUF support — rebuild with `--features local-llm`, or use --endpoint",
    ))
}

fn print_report(p: &advisor::Profile) {
    println!("Architecture profile — subscription {}", p.subscription);
    println!(
        "  Resources: {} across {} resource groups",
        p.resource_count, p.resource_groups
    );

    if !p.services.is_empty() {
        println!("  Top services:");
        for c in p.services.iter().take(8) {
            let short = c.name.rsplit('/').next().unwrap_or(&c.name);
            println!("    {:>4}  {short}", c.count);
        }
    }
    if !p.regions.is_empty() {
        let regions = p
            .regions
            .iter()
            .map(|c| format!("{} ({})", c.name, c.count))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  Regions: {regions}");
    }
    println!(
        "  CAF naming: {}% conform ({}/{})",
        p.naming.percent, p.naming.conforming, p.naming.total
    );
    let tags = p
        .tagging
        .iter()
        .map(|t| format!("{} {}%", t.tag, t.percent))
        .collect::<Vec<_>>()
        .join(", ");
    println!("  Required tags: {tags}");
    println!(
        "  Governance: {} policy assignment(s), {} non-compliant",
        p.governance.assignments, p.governance.non_compliant
    );

    if !p.signals.is_empty() {
        println!("\nSignals:");
        for s in &p.signals {
            println!("  • {s}");
        }
    }
    if p.warnings.is_empty() {
        println!("\n✓ No guardrail warnings.");
    } else {
        println!("\nWarnings:");
        for w in &p.warnings {
            println!("  ⚠ {w}");
        }
    }
    println!("\n(Add `--summary --endpoint <url>` for a natural-language AI review.)");
}
