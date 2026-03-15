//! The `eval` command - evaluate modules and output configuration.
//!
//! This command evaluates Nix modules using the library's high-level API
//! and outputs the resulting configuration in the specified format.

use super::{collect_files, format_value, print_diagnostics, write_output};
use crate::{exit_codes, Cli};
use nix_module_system::api::{ApiError, ModuleEvaluator};
use nix_module_system::errors::SourceCache;
// Note: OptionPath and Value are used implicitly through the API
use std::path::PathBuf;

/// Run the eval command.
pub fn run(
    files: &[PathBuf],
    attr: Option<&str>,
    _raw: bool,
    cli: &Cli,
) -> anyhow::Result<u8> {
    // Collect all .nix files
    let file_paths = collect_files(files)?;

    if !cli.quiet {
        tracing::debug!("Evaluating {} files", file_paths.len());
        for path in &file_paths {
            tracing::debug!("  - {}", path.display());
        }
    }

    // Build the evaluator
    let mut evaluator = ModuleEvaluator::new();

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
        Err(ApiError::Parse { file, errors }) => {
            if !cli.quiet {
                eprintln!("error: Parse error in {}", file.display());
                for err in &errors {
                    eprintln!("  {}", err);
                }
            }
            return Ok(exit_codes::EVAL_ERROR);
        }
        Err(ApiError::Io { path, message }) => {
            if !cli.quiet {
                eprintln!("error: IO error for {}: {}", path.display(), message);
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

    // Check for errors/warnings in strict mode
    if cli.strict && config.has_warnings() {
        if !cli.quiet {
            eprintln!("error: Evaluation completed with warnings (--strict mode)");
        }
        return Ok(exit_codes::EVAL_ERROR);
    }

    if config.has_errors() {
        return Ok(exit_codes::EVAL_ERROR);
    }

    // Extract the value to output
    let output_value = if let Some(attr_path) = attr {
        match config.get_raw(attr_path) {
            Some(v) => v.clone(),
            None => {
                if !cli.quiet {
                    eprintln!("error: Attribute '{}' not found in configuration", attr_path);

                    // Suggest similar paths
                    let options: Vec<_> = config.options().map(|o| o.path.to_dotted()).collect();
                    let similar = find_similar_paths(attr_path, &options);
                    if !similar.is_empty() {
                        eprintln!("\nDid you mean one of these?");
                        for s in similar {
                            eprintln!("  - {}", s);
                        }
                    }
                }
                return Ok(exit_codes::EVAL_ERROR);
            }
        }
    } else {
        config.config().clone()
    };

    // Format and output
    let formatted = format_value(&output_value, cli.format)?;
    write_output(&formatted, cli)?;

    Ok(exit_codes::SUCCESS)
}

/// Find similar paths using simple prefix matching.
fn find_similar_paths(target: &str, candidates: &[String]) -> Vec<String> {
    let target_parts: Vec<_> = target.split('.').collect();

    candidates
        .iter()
        .filter(|c| {
            let parts: Vec<_> = c.split('.').collect();
            // Match if first component is similar
            if parts.is_empty() || target_parts.is_empty() {
                return false;
            }
            parts[0] == target_parts[0] ||
            levenshtein(parts[0], target_parts[0]) <= 2
        })
        .take(5)
        .cloned()
        .collect()
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

    #[test]
    fn test_find_similar_paths() {
        let candidates = vec![
            "services.nginx.enable".to_string(),
            "services.nginx.port".to_string(),
            "services.mysql.enable".to_string(),
            "networking.firewall.enable".to_string(),
        ];

        let similar = find_similar_paths("services.nginx.enabled", &candidates);
        assert!(similar.contains(&"services.nginx.enable".to_string()));
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("hello", "hello"), 0);
        assert_eq!(levenshtein("hello", "helo"), 1);
        assert_eq!(levenshtein("enabled", "enable"), 1);
    }
}
