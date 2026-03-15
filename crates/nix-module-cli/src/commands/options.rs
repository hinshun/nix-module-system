//! The `options` command - list declared options with metadata.
//!
//! This command analyzes Nix modules and outputs information about
//! declared options including their types, defaults, and descriptions.

use super::{collect_files, format_output, print_diagnostics, write_output};
use crate::{exit_codes, Cli, OutputFormat};
use nix_module_system::api::{ApiError, ModuleEvaluator};
use nix_module_system::errors::SourceCache;
use nix_module_system::types::Value;
// indexmap is used via nix-module-system
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
    /// Default value (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Description (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Where the option was declared.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub declared_in: Vec<String>,
}

/// Run the options command.
pub fn run(
    files: &[PathBuf],
    prefix: Option<&str>,
    include_internal: bool,
    paths_only: bool,
    cli: &Cli,
) -> anyhow::Result<u8> {
    // Collect all .nix files
    let file_paths = collect_files(files)?;

    if !cli.quiet {
        tracing::debug!("Analyzing {} files for options", file_paths.len());
    }

    // Build the evaluator
    let mut evaluator = ModuleEvaluator::new()
        .include_internal(include_internal);

    for path in &file_paths {
        evaluator = evaluator.add_file(path)?;
    }

    // Run evaluation
    let config = match evaluator.evaluate() {
        Ok(cfg) => cfg,
        Err(ApiError::Eval(e)) => {
            if !cli.quiet {
                eprintln!("error: {}", e);
            }
            return Ok(exit_codes::EVAL_ERROR);
        }
        Err(e) => {
            if !cli.quiet {
                eprintln!("error: {}", e);
            }
            return Ok(exit_codes::EVAL_ERROR);
        }
    };

    // Print any diagnostics
    let mut cache = SourceCache::new();
    print_diagnostics(config.diagnostics(), &mut cache, cli.quiet);

    if config.has_errors() {
        return Ok(exit_codes::EVAL_ERROR);
    }

    // Collect options
    let options: Vec<OptionOutput> = config
        .options()
        .filter(|opt| {
            // Filter by prefix if specified
            if let Some(p) = prefix {
                opt.path.to_dotted().starts_with(p)
            } else {
                true
            }
        })
        .map(|opt| OptionOutput {
            path: opt.path.to_dotted(),
            type_desc: opt.type_desc.clone(),
            default: opt.default.clone(),
            description: opt.description.clone(),
            declared_in: opt.declared_in.iter().map(|p| p.display().to_string()).collect(),
        })
        .collect();

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

    // Format output
    if paths_only {
        // Simple list of paths
        let paths: Vec<&str> = options.iter().map(|o| o.path.as_str()).collect();
        let output = paths.join("\n");
        write_output(&output, cli)?;
    } else {
        // Full option information
        match cli.format {
            OutputFormat::Nix => {
                // Format as Nix attrset
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
            result.push_str(&format!("    default = {};\n", format_value_nix(default)));
        }

        if !opt.declared_in.is_empty() {
            result.push_str("    declaredIn = [\n");
            for path in &opt.declared_in {
                result.push_str(&format!("      \"{}\"\n", escape_nix(path)));
            }
            result.push_str("    ];\n");
        }

        result.push_str("  };\n");
    }

    result.push('}');
    result
}

/// Format a value as Nix (simple version).
fn format_value_nix(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("\"{}\"", escape_nix(s)),
        Value::Path(p) => p.display().to_string(),
        Value::List(items) => {
            let inner: Vec<_> = items.iter().map(format_value_nix).collect();
            format!("[ {} ]", inner.join(" "))
        }
        Value::Attrs(attrs) => {
            let inner: Vec<_> = attrs
                .iter()
                .map(|(k, v)| format!("{} = {};", k, format_value_nix(v)))
                .collect();
            format!("{{ {} }}", inner.join(" "))
        }
        Value::Lambda => "<lambda>".to_string(),
        Value::Derivation(_) => "<derivation>".to_string(),
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
            default: Some(Value::Bool(false)),
            description: Some("Enable the test service".to_string()),
            declared_in: vec!["test.nix".to_string()],
        }];

        let output = format_options_as_nix(&options);
        assert!(output.contains("test.enable"));
        assert!(output.contains("bool"));
        assert!(output.contains("Enable the test service"));
    }
}
