//! The `check` command - validate modules without full evaluation.
//!
//! Checks Nix modules for errors by running evalModules and verifying the
//! result evaluates successfully.

use super::collect_files;
use crate::nix_eval::{self, NixEvalConfig};
use crate::{exit_codes, Cli};
use std::path::PathBuf;

/// Run the check command.
pub fn run(
    files: &[PathBuf],
    _warnings_as_errors: bool,
    cli: &Cli,
    eval_config: &NixEvalConfig,
) -> anyhow::Result<u8> {
    let file_paths = collect_files(files)?;

    if !cli.quiet {
        eprintln!("Checking {} files...", file_paths.len());
    }

    let mut total_errors = 0;
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

    if valid_files.is_empty() {
        if !cli.quiet {
            eprintln!("error: No valid files to check");
        }
        return Ok(exit_codes::EVAL_ERROR);
    }

    match nix_eval::check_modules(eval_config, &valid_files) {
        Ok(()) => {
            if !cli.quiet && total_errors == 0 {
                eprintln!("All {} files checked successfully.", file_paths.len());
            }
        }
        Err(e) => {
            if !cli.quiet {
                eprintln!("error: {}", e);
            }
            total_errors += 1;
        }
    }

    if total_errors > 0 {
        Ok(exit_codes::EVAL_ERROR)
    } else {
        Ok(exit_codes::SUCCESS)
    }
}
