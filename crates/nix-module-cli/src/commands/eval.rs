//! The `eval` command - evaluate modules and output configuration.
//!
//! Evaluates Nix modules by calling `evalModules` via the embedded Nix
//! evaluator with the plugin loaded.

use super::{collect_files, format_value, write_output};
use crate::nix_eval::{self, NixEvalConfig};
use crate::{exit_codes, Cli};
use nix_module_system::types::Value;
use std::path::PathBuf;

/// Run the eval command.
pub fn run(
    files: &[PathBuf],
    attr: Option<&str>,
    _raw: bool,
    cli: &Cli,
    eval_config: &NixEvalConfig,
) -> anyhow::Result<u8> {
    let file_paths = collect_files(files)?;

    if !cli.quiet {
        tracing::debug!("Evaluating {} files", file_paths.len());
        for path in &file_paths {
            tracing::debug!("  - {}", path.display());
        }
    }

    let result = match nix_eval::eval_modules(eval_config, &file_paths) {
        Ok(r) => r,
        Err(e) => {
            if !cli.quiet {
                eprintln!("error: {}", e);
            }
            return Ok(exit_codes::EVAL_ERROR);
        }
    };

    let output_value = if let Some(attr_path) = attr {
        match get_attr_path(&result.config, attr_path) {
            Some(v) => v.clone(),
            None => {
                if !cli.quiet {
                    eprintln!("error: Attribute '{}' not found in configuration", attr_path);

                    if let Value::Attrs(ref attrs) = result.config {
                        let first_component = attr_path.split('.').next().unwrap_or("");
                        let similar: Vec<_> = attrs
                            .keys()
                            .filter(|k| levenshtein(k, first_component) <= 2)
                            .take(5)
                            .collect();
                        if !similar.is_empty() {
                            eprintln!("\nDid you mean one of these?");
                            for s in similar {
                                eprintln!("  - {}", s);
                            }
                        }
                    }
                }
                return Ok(exit_codes::EVAL_ERROR);
            }
        }
    } else {
        result.config
    };

    let formatted = format_value(&output_value, cli.format)?;
    write_output(&formatted, cli)?;

    Ok(exit_codes::SUCCESS)
}

/// Navigate into a Value by a dotted attribute path.
fn get_attr_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for component in path.split('.') {
        match current {
            Value::Attrs(attrs) => {
                current = attrs.get(component)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Simple Levenshtein distance.
fn levenshtein(a: &str, b: &str) -> usize {
    if a.is_empty() { return b.len(); }
    if b.is_empty() { return a.len(); }

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1)
                .min(curr[j] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_get_attr_path() {
        let mut inner = IndexMap::new();
        inner.insert("enable".to_string(), Value::Bool(true));
        let mut outer = IndexMap::new();
        outer.insert("nginx".to_string(), Value::Attrs(inner));
        let mut root = IndexMap::new();
        root.insert("services".to_string(), Value::Attrs(outer));
        let value = Value::Attrs(root);

        assert_eq!(
            get_attr_path(&value, "services.nginx.enable"),
            Some(&Value::Bool(true))
        );
        assert_eq!(get_attr_path(&value, "services.missing"), None);
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("hello", "hello"), 0);
        assert_eq!(levenshtein("hello", "helo"), 1);
    }
}
