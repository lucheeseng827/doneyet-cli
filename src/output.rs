use anyhow::Result;
use comfy_table::{ContentArrangement, Table};
use serde::Serialize;
use serde_json::Value;

use crate::cli::OutputFormat;

pub fn print<T: Serialize>(fmt: OutputFormat, value: &T) -> Result<()> {
    match fmt {
        OutputFormat::Json => print_json(value),
        OutputFormat::Table => {
            // Fall back to JSON for arbitrary serializable types — specific
            // commands that want a real table call print_table directly.
            print_json(value)
        }
    }
}

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let s = serde_json::to_string_pretty(value)?;
    println!("{s}");
    Ok(())
}

/// Render an array of JSON objects as a table by picking the requested columns.
pub fn print_table(
    fmt: OutputFormat,
    rows: &[Value],
    columns: &[&str],
) -> Result<()> {
    match fmt {
        OutputFormat::Json => print_json(&rows),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(columns.iter().copied().map(uppercase));
            for row in rows {
                let cells: Vec<String> = columns
                    .iter()
                    .map(|c| stringify(row.get(*c).unwrap_or(&Value::Null)))
                    .collect();
                table.add_row(cells);
            }
            println!("{table}");
            Ok(())
        }
    }
}

/// `rows` is a list of `(label, field-name)` pairs. The field name is a
/// single top-level key on `value` (`value.get(field)`); nested paths like
/// `"foo.bar"` are NOT supported.
pub fn print_object_table(
    fmt: OutputFormat,
    value: &Value,
    rows: &[(&str, &str)],
) -> Result<()> {
    match fmt {
        OutputFormat::Json => print_json(value),
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(["FIELD", "VALUE"]);
            for (label, path) in rows {
                table.add_row([
                    label.to_string(),
                    stringify(value.get(path).unwrap_or(&Value::Null)),
                ]);
            }
            println!("{table}");
            Ok(())
        }
    }
}

fn uppercase(s: &str) -> String {
    s.to_ascii_uppercase()
}

fn stringify(v: &Value) -> String {
    match v {
        Value::Null => "-".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(_) | Value::Object(_) => v.to_string(),
    }
}
