//! Output formatting: json / table / tsv, mirroring az's `--output` modes.
//!
//! az lets each command supply a "table transformer" that projects a result into
//! columns. We do the lighter-weight equivalent: a command returns a [`serde_json::Value`]
//! and an optional ordered list of columns to project for table/tsv rendering.

use comfy_table::{presets::UTF8_FULL, Table};
use serde_json::Value;
use std::str::FromStr;

use crate::error::{usage, Result};

/// Output format selected via `--output/-o`. JSON is the default, as in az.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    Yaml,
    Table,
    Tsv,
    /// Suppress result output entirely (az `--output none`).
    None,
}

impl FromStr for OutputFormat {
    type Err = crate::error::RazError;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "yaml" => Ok(OutputFormat::Yaml),
            "table" => Ok(OutputFormat::Table),
            "tsv" => Ok(OutputFormat::Tsv),
            "none" => Ok(OutputFormat::None),
            other => Err(usage(format!(
                "unknown output format '{other}' (expected json|yaml|table|tsv|none)"
            ))),
        }
    }
}

/// Table layout: ordered `(column header, top-level JSON key)` pairs looked up on each row.
pub type TableSpec = Vec<(&'static str, &'static str)>;

/// Render `value` in the requested format. `table` is consulted for table/tsv modes;
/// when absent we fall back to pretty JSON so no command is ever unprintable.
pub fn render(value: &Value, format: OutputFormat, table: Option<&TableSpec>) -> Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(value)?),
        OutputFormat::Yaml => serde_yaml::to_string(value)
            .map_err(|e| crate::error::RazError::Other(format!("yaml: {e}"))),
        OutputFormat::Table => match table {
            Some(spec) => Ok(render_table(value, spec)),
            None => Ok(serde_json::to_string_pretty(value)?),
        },
        OutputFormat::Tsv => match table {
            Some(spec) => Ok(render_tsv(value, spec)),
            None => Ok(tsv_scalar(value)),
        },
        OutputFormat::None => Ok(String::new()),
    }
}

/// Normalize a value into a list of row objects: a top-level array stays as is,
/// a single object becomes a one-row list.
fn rows(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    }
}

fn cell(row: &Value, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn render_table(value: &Value, spec: &TableSpec) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(spec.iter().map(|(h, _)| *h).collect::<Vec<_>>());
    for row in rows(value) {
        table.add_row(spec.iter().map(|(_, k)| cell(row, k)).collect::<Vec<_>>());
    }
    table.to_string()
}

fn render_tsv(value: &Value, spec: &TableSpec) -> String {
    rows(value)
        .iter()
        .map(|row| {
            spec.iter()
                .map(|(_, k)| cell(row, k))
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tsv_scalar(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Apply a JMESPath `--query` to the result, like az. Supports the full JMESPath grammar
/// (projections, filters, functions, pipes). An empty query returns the value unchanged; an
/// invalid expression or no match yields JSON null.
pub fn apply_query(value: &Value, query: &str) -> Value {
    if query.trim().is_empty() {
        return value.clone();
    }
    let result = jmespath::compile(query).and_then(|expr| expr.search(value));
    match result {
        Ok(var) => serde_json::to_value(&*var).unwrap_or(Value::Null),
        Err(_) => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec() -> TableSpec {
        vec![("Name", "name"), ("Location", "location")]
    }

    #[test]
    fn parses_formats_case_insensitively() {
        assert_eq!("JSON".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!(
            "Table".parse::<OutputFormat>().unwrap(),
            OutputFormat::Table
        );
        assert_eq!("YAML".parse::<OutputFormat>().unwrap(), OutputFormat::Yaml);
        assert!("xml".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn json_is_pretty_by_default() {
        let v = json!({"name": "vm1"});
        let out = render(&v, OutputFormat::Json, None).unwrap();
        assert!(out.contains("\"name\": \"vm1\""));
    }

    #[test]
    fn tsv_projects_columns_for_array() {
        let v = json!([
            {"name": "vm1", "location": "westus"},
            {"name": "vm2", "location": "eastus"}
        ]);
        let out = render(&v, OutputFormat::Tsv, Some(&spec())).unwrap();
        assert_eq!(out, "vm1\twestus\nvm2\teastus");
    }

    #[test]
    fn table_includes_headers_and_rows() {
        let v = json!([{"name": "vm1", "location": "westus"}]);
        let out = render(&v, OutputFormat::Table, Some(&spec())).unwrap();
        assert!(out.contains("Name"));
        assert!(out.contains("vm1"));
        assert!(out.contains("westus"));
    }

    #[test]
    fn missing_keys_render_empty() {
        let v = json!([{"name": "vm1"}]);
        let out = render(&v, OutputFormat::Tsv, Some(&spec())).unwrap();
        assert_eq!(out, "vm1\t");
    }

    #[test]
    fn query_runs_jmespath() {
        let v = json!({"subs": [{"name": "Dev"}, {"name": "Prod"}]});
        assert_eq!(apply_query(&v, "subs[1].name"), json!("Prod"));
        assert_eq!(apply_query(&v, "subs[].name"), json!(["Dev", "Prod"]));
        assert_eq!(apply_query(&v, "subs[5]"), Value::Null);
        assert_eq!(apply_query(&v, ""), v);
    }
}
