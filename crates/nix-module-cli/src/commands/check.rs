//! The `check` command - validate modules without full evaluation.
//!
//! This command checks Nix modules for syntax and type errors without
//! performing full evaluation. It's faster and useful for CI pipelines.

use super::{collect_files, print_diagnostics};
use crate::{exit_codes, Cli};
use nix_module_system::api::{ApiError, ModuleEvaluator};
use nix_module_system::errors::{Severity, SourceCache};
use std::path::PathBuf;

/// Run the check command.
pub fn run(
    files: &[PathBuf],
    warnings_as_errors: bool,
    cli: &Cli,
) -> anyhow::Result<u8> {
    // Collect all .nix files
    let file_paths = collect_files(files)?;

    if !cli.quiet {
        tracing::info!("Checking {} files...", file_paths.len());
    }

    // Track results
    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut cache = SourceCache::new();

    // First pass: check which files are readable
    let mut valid_files = Vec::new();
    for path in &file_paths {
        if path.exists() {
            valid_files.push(path.clone());
        } else {
            if !cli.quiet {
                eprintln!("error: File not found: {}", path.display());
            }
            total_errors += 1;
        }
    }

    // If no valid files remain, exit early
    if valid_files.is_empty() {
        if !cli.quiet {
            eprintln!("error: No valid files to check");
        }
        return Ok(exit_codes::EVAL_ERROR);
    }

    // Build the evaluator with valid files
    let mut evaluator = ModuleEvaluator::new()
        .lenient(true);  // Continue on parse errors to collect all issues

    for path in &valid_files {
        match evaluator.add_file(path) {
            Ok(e) => evaluator = e,
            Err(ApiError::Io { path: p, message }) => {
                if !cli.quiet {
                    eprintln!("error: Cannot read {}: {}", p.display(), message);
                }
                total_errors += 1;
                // Create fresh evaluator to continue
                evaluator = ModuleEvaluator::new().lenient(true);
            }
            Err(e) => {
                if !cli.quiet {
                    eprintln!("error: {}", e);
                }
                total_errors += 1;
                // Create fresh evaluator to continue
                evaluator = ModuleEvaluator::new().lenient(true);
            }
        }
    }

    // Run evaluation to catch all errors
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
                eprintln!("error: Parse errors in {}", file.display());
                for err in &errors {
                    eprintln!("  {}", err);
                }
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

    // Count and display diagnostics
    for diag in config.diagnostics() {
        match diag.severity {
            Severity::Error => total_errors += 1,
            Severity::Warning => total_warnings += 1,
            _ => {}
        }
    }

    // Print all diagnostics
    print_diagnostics(config.diagnostics(), &mut cache, cli.quiet);

    // Summary
    if !cli.quiet {
        if total_errors == 0 && total_warnings == 0 {
            eprintln!("All {} files checked successfully.", file_paths.len());
        } else {
            eprintln!();
            eprintln!(
                "Checked {} files: {} errors, {} warnings",
                file_paths.len(),
                total_errors,
                total_warnings
            );
        }
    }

    // Determine exit code
    let treat_warnings_as_errors = warnings_as_errors || cli.strict;

    if total_errors > 0 {
        Ok(exit_codes::EVAL_ERROR)
    } else if treat_warnings_as_errors && total_warnings > 0 {
        Ok(exit_codes::EVAL_ERROR)
    } else {
        Ok(exit_codes::SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    // Tests would require actual Nix files, which we'll skip for now
}
