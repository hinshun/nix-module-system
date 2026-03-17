//! The `options` command - list declared options with metadata.
//!
//! Analyzes Nix modules by evaluating them and extracting option declarations
//! including types, defaults, and descriptions.

use super::{collect_files, format_output, write_output};
use crate::nix_eval::{self, NixEvalConfig};
use crate::{exit_codes, Cli, OutputFormat};
use nix_module_system::nix::value_to_json;
use nix_module_system::types::Value;
use serde::Serialize;
use std::path::PathBuf;

/// Information about an option for output.
#[derive(Debug, Serialize)]
pub struct OptionOutput {
    /// The option path (e.g., "services.nginx.enable").
    pub path: String,
    /// Type description.
    #[serde(rename = "type")]
    pub type_desc: String,
    /// Default value (if any), serialized as plain JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Description (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this is an internal option.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub internal: bool,
}

/// Run the options command.
pub fn run(
    files: &[PathBuf],
    prefix: Option<&str>,
    include_internal: bool,
    paths_only: bool,
    cli: &Cli,
    eval_config: &NixEvalConfig,
) -> anyhow::Result<u8> {
    let file_paths = collect_files(files)?;

    if !cli.quiet {
        tracing::debug!("Analyzing {} files for options", file_paths.len());
    }

    let options_value = match nix_eval::eval_options(eval_config, &file_paths) {
        Ok(v) => v,
        Err(e) => {
            if !cli.quiet {
                eprintln!("error: {}", e);
            }
            return Ok(exit_codes::EVAL_ERROR);
        }
    };

    let options = extract_options(&options_value, prefix, include_internal);

    if options.is_empty() {
        if !cli.quiet {
            if let Some(p) = prefix {
                eprintln!("No options found with prefix '{}'", p);
            } else {
                eprintln!("No options declared in the specified modules");
            }
        }
        return Ok(exit_codes::SUCCESS);
    }

    if paths_only {
        let paths: Vec<&str> = options.iter().map(|o| o.path.as_str()).collect();
        let output = paths.join("\n");
        write_output(&output, cli)?;
    } else {
        match cli.format {
            OutputFormat::Nix => {
                let output = format_options_as_nix(&options);
                write_output(&output, cli)?;
            }
            _ => {
                let output = format_output(&options, cli.format)?;
                write_output(&output, cli)?;
            }
        }
    }

    if !cli.quiet {
        eprintln!("Found {} options", options.len());
    }

    Ok(exit_codes::SUCCESS)
}

/// Extract option information from the Nix evaluation result.
fn extract_options(
    value: &Value,
    prefix: Option<&str>,
    include_internal: bool,
) -> Vec<OptionOutput> {
    let mut options = Vec::new();

    if let Value::Attrs(attrs) = value {
        for (name, opt_value) in attrs {
            if let Some(p) = prefix {
                if !name.starts_with(p) {
                    continue;
                }
            }

            if let Value::Attrs(opt_attrs) = opt_value {
                let internal = matches!(opt_attrs.get("internal"), Some(Value::Bool(true)));

                if internal && !include_internal {
                    continue;
                }

                let type_desc = match opt_attrs.get("type") {
                    Some(Value::String(s)) => s.clone(),
                    _ => "unknown".to_string(),
                };

                let default = opt_attrs.get("default").and_then(|v| {
                    if matches!(v, Value::Null) { None } else { Some(value_to_json(v)) }
                });

                let description = match opt_attrs.get("description") {
                    Some(Value::String(s)) => Some(s.clone()),
                    _ => None,
                };

                options.push(OptionOutput {
                    path: name.clone(),
                    type_desc,
                    default,
                    description,
                    internal,
                });
            }
        }
    }

    options
}

/// Format options as a Nix expression.
fn format_options_as_nix(options: &[OptionOutput]) -> String {
    let mut result = String::from("{\n");

    for opt in options {
        result.push_str(&format!("  \"{}\" = {{\n", opt.path));
        result.push_str(&format!("    type = \"{}\";\n", escape_nix(&opt.type_desc)));

        if let Some(ref desc) = opt.description {
            result.push_str(&format!("    description = \"{}\";\n", escape_nix(desc)));
        }

        if let Some(ref default) = opt.default {
            result.push_str(&format!("    default = {};\n", format_json_as_nix(default)));
        }

        result.push_str("  };\n");
    }

    result.push('}');
    result
}

/// Format a JSON value as Nix syntax.
fn format_json_as_nix(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("\"{}\"", escape_nix(s)),
        serde_json::Value::Array(items) => {
            let inner: Vec<_> = items.iter().map(format_json_as_nix).collect();
            format!("[ {} ]", inner.join(" "))
        }
        serde_json::Value::Object(attrs) => {
            let inner: Vec<_> = attrs
                .iter()
                .map(|(k, v)| format!("{} = {};", k, format_json_as_nix(v)))
                .collect();
            format!("{{ {} }}", inner.join(" "))
        }
    }
}

/// Escape a string for Nix.
fn escape_nix(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('$', "\\$")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_nix() {
        assert_eq!(escape_nix("hello"), "hello");
        assert_eq!(escape_nix("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_nix("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn test_format_options_as_nix() {
        let options = vec![OptionOutput {
            path: "test.enable".to_string(),
            type_desc: "bool".to_string(),
            default: Some(serde_json::Value::Bool(false)),
            description: Some("Enable the test service".to_string()),
            internal: false,
        }];

        let output = format_options_as_nix(&options);
        assert!(output.contains("test.enable"));
        assert!(output.contains("bool"));
        assert!(output.contains("Enable the test service"));
    }
}
