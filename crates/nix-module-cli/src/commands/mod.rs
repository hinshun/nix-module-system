//! CLI command implementations.
//!
//! Each command is implemented in its own module:
//! - `eval` - Evaluate modules and output configuration
//! - `check` - Check modules for errors
//! - `options` - List declared options

pub mod check;
pub mod eval;
pub mod options;

use crate::{Cli, OutputFormat};
use nix_module_system::nix::value_to_json;
use nix_module_system::types::Value;
use serde::Serialize;
use std::io::{self, Write};
use std::path::PathBuf;

/// Write output to the appropriate destination (stdout or file).
pub fn write_output(output: &str, cli: &Cli) -> io::Result<()> {
    match &cli.output {
        Some(path) => {
            std::fs::write(path, output)?;
            if !cli.quiet {
                eprintln!("Output written to: {}", path.display());
            }
            Ok(())
        }
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            writeln!(handle, "{}", output)?;
            Ok(())
        }
    }
}

/// Format a value according to the output format.
pub fn format_value(value: &Value, format: OutputFormat) -> anyhow::Result<String> {
    match format {
        OutputFormat::Json => {
            let json = value_to_json(value);
            Ok(serde_json::to_string_pretty(&json)?)
        }
        OutputFormat::Yaml => {
            let json = value_to_json(value);
            Ok(serde_yaml::to_string(&json)?)
        }
        OutputFormat::Nix => {
            Ok(format_as_nix(value))
        }
    }
}

/// Format any serializable value according to the output format.
pub fn format_output<T: Serialize>(value: &T, format: OutputFormat) -> anyhow::Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(value)?),
        OutputFormat::Yaml => Ok(serde_yaml::to_string(value)?),
        OutputFormat::Nix => Ok(serde_json::to_string_pretty(value)?),
    }
}

/// Format a Value as Nix expression syntax.
fn format_as_nix(value: &Value) -> String {
    format_as_nix_indent(value, 0)
}

fn format_as_nix_indent(value: &Value, indent: usize) -> String {
    let spaces = "  ".repeat(indent);
    let inner_spaces = "  ".repeat(indent + 1);

    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            let s = f.to_string();
            if s.contains('.') || s.contains('e') || s.contains('E') { s }
            else { format!("{}.0", s) }
        }
        Value::String(s) => format!("\"{}\"", escape_nix_string(s)),
        Value::Path(p) => p.display().to_string(),
        Value::List(items) => {
            if items.is_empty() {
                "[ ]".to_string()
            } else if items.len() == 1 && is_simple_value(&items[0]) {
                format!("[ {} ]", format_as_nix_indent(&items[0], 0))
            } else {
                let mut result = String::from("[\n");
                for item in items {
                    result.push_str(&inner_spaces);
                    result.push_str(&format_as_nix_indent(item, indent + 1));
                    result.push('\n');
                }
                result.push_str(&spaces);
                result.push(']');
                result
            }
        }
        Value::Attrs(attrs) => {
            if attrs.is_empty() {
                "{ }".to_string()
            } else {
                let mut result = String::from("{\n");
                for (key, val) in attrs {
                    result.push_str(&inner_spaces);
                    if needs_quoting(key) {
                        result.push_str(&format!("\"{}\"", escape_nix_string(key)));
                    } else {
                        result.push_str(key);
                    }
                    result.push_str(" = ");
                    result.push_str(&format_as_nix_indent(val, indent + 1));
                    result.push_str(";\n");
                }
                result.push_str(&spaces);
                result.push('}');
                result
            }
        }
        Value::Lambda => "<lambda>".to_string(),
        Value::Derivation(d) => {
            format!("/* derivation */ {}", format_as_nix_indent(d, indent))
        }
    }
}

fn is_simple_value(value: &Value) -> bool {
    matches!(value, Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_) | Value::Path(_))
}

fn needs_quoting(name: &str) -> bool {
    if name.is_empty() { return true; }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' { return true; }
    !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '\'')
}

fn escape_nix_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '$' => result.push_str("\\$"),
            c => result.push(c),
        }
    }
    result
}

/// Collect files from paths, expanding directories.
pub fn collect_files(paths: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        if path.is_dir() {
            for entry in walkdir(path)? {
                if entry.extension().map(|e| e == "nix").unwrap_or(false) {
                    files.push(entry);
                }
            }
        } else if path.exists() {
            files.push(std::fs::canonicalize(path)?);
        } else {
            anyhow::bail!("File not found: {}", path.display());
        }
    }

    if files.is_empty() {
        anyhow::bail!("No .nix files found in the specified paths");
    }

    Ok(files)
}

fn walkdir(dir: &PathBuf) -> io::Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            results.extend(walkdir(&path)?);
        } else {
            results.push(std::fs::canonicalize(&path)?);
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_escape_nix_string() {
        assert_eq!(escape_nix_string("hello"), "hello");
        assert_eq!(escape_nix_string("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_nix_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_nix_string("path $HOME"), "path \\$HOME");
    }

    #[test]
    fn test_needs_quoting() {
        assert!(!needs_quoting("hello"));
        assert!(!needs_quoting("hello_world"));
        assert!(!needs_quoting("hello-world"));
        assert!(needs_quoting("123abc"));
        assert!(needs_quoting("hello world"));
        assert!(needs_quoting(""));
    }

    #[test]
    fn test_format_as_nix_simple() {
        assert_eq!(format_as_nix(&Value::Null), "null");
        assert_eq!(format_as_nix(&Value::Bool(true)), "true");
        assert_eq!(format_as_nix(&Value::Int(42)), "42");
        assert_eq!(format_as_nix(&Value::String("hello".into())), "\"hello\"");
    }

    #[test]
    fn test_format_as_nix_list() {
        let list = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let formatted = format_as_nix(&list);
        assert!(formatted.contains('['));
        assert!(formatted.contains(']'));
    }

    #[test]
    fn test_format_as_nix_attrs() {
        let mut attrs = IndexMap::new();
        attrs.insert("foo".to_string(), Value::Bool(true));
        let formatted = format_as_nix(&Value::Attrs(attrs));
        assert!(formatted.contains("foo"));
        assert!(formatted.contains("true"));
    }
}
