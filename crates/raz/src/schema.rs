//! `raz __schema`: dump the clap command tree as JSON so `raz-tui` can offer context-aware
//! autocomplete without duplicating the flag definitions. The schema is derived from the live
//! clap `Command`, so it never drifts from the actual CLI.

use clap::{Arg, ArgAction, Command, CommandFactory};
use serde_json::{json, Value};

/// Print the schema for clap command `C` as a JSON array of leaf commands.
pub fn print<C: CommandFactory>() {
    let root = C::command();
    let globals: Vec<Value> = root.get_arguments().filter_map(global_flag).collect();
    let mut out = Vec::new();
    collect(&root, "", &globals, &mut out);
    println!("{}", Value::Array(out));
}

/// Recurse into subcommands, emitting one JSON object per leaf command (a command with no
/// further subcommands).
fn collect(cmd: &Command, prefix: &str, globals: &[Value], out: &mut Vec<Value>) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" || name.starts_with("__") {
            continue;
        }
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix} {name}")
        };

        if sub.get_subcommands().next().is_some() {
            collect(sub, &path, globals, out);
            continue;
        }

        let mut flags: Vec<Value> = sub.get_arguments().filter_map(flag_json).collect();
        for g in globals {
            if !flags.iter().any(|f| f.get("long") == g.get("long")) {
                flags.push(g.clone());
            }
        }
        out.push(json!({
            "path": path,
            "about": sub.get_about().map(|a| a.to_string()).unwrap_or_default(),
            "flags": flags,
        }));
    }
}

/// JSON for a single argument, or `None` for positionals and the auto help/version flags.
fn flag_json(arg: &Arg) -> Option<Value> {
    let long = arg.get_long()?;
    if matches!(arg.get_id().as_str(), "help" | "version") {
        return None;
    }
    Some(json!({
        "long": long,
        "short": arg.get_short().map(|c| c.to_string()),
        "takes_value": matches!(arg.get_action(), ArgAction::Set | ArgAction::Append),
        "required": arg.is_required_set(),
        "help": arg.get_help().map(|h| h.to_string()).unwrap_or_default(),
    }))
}

/// Same as [`flag_json`] but only for global args (those propagated to every subcommand).
fn global_flag(arg: &Arg) -> Option<Value> {
    if arg.is_global_set() {
        flag_json(arg)
    } else {
        None
    }
}
