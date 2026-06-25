//! `raz configure` — view or set persisted defaults (group / location / output), the az
//! `configure --defaults` / `config set defaults.*` analogue. Stored in `~/.raz/profile.json`.

use clap::Args;
use serde_json::json;

use raz_core::config::Profile;
use raz_core::error::Result;
use raz_core::GlobalArgs;

use super::emit;

#[derive(Args)]
pub struct ConfigureArgs {
    /// Default region for `create` commands. Pass "" to clear.
    #[arg(long)]
    default_location: Option<String>,
    /// Default output format (json/yaml/table/tsv/none). Pass "" to clear.
    #[arg(long)]
    default_output: Option<String>,
}

/// Set the given fields (an empty string clears one), then print the resulting defaults. With no
/// flags, just prints the current defaults.
pub fn run(args: ConfigureArgs, globals: GlobalArgs) -> Result<()> {
    let mut profile = Profile::load()?;

    let set = |slot: &mut Option<String>, val: Option<String>| {
        if let Some(v) = val {
            *slot = if v.is_empty() { None } else { Some(v) };
        }
    };
    set(&mut profile.defaults.location, args.default_location);
    set(&mut profile.defaults.output, args.default_output);
    profile.save()?;

    let d = &profile.defaults;
    let value = json!({
        "location": d.location.clone().unwrap_or_default(),
        "output": d.output.clone().unwrap_or_default(),
    });
    // Reuse the renderer; build a minimal context just for output settings.
    let ctx = raz_core::context::Context::load(globals)?;
    emit(
        &ctx,
        value,
        Some(&vec![("Location", "location"), ("Output", "output")]),
    )
}
