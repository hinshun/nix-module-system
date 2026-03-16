//! CLI for the Nix module system.
//!
//! A thin CLI wrapper around the nix-module-system library.
//!
//! # Commands
//!
//! - `nix-module eval <files...>` - Evaluate modules, output config as JSON/Nix
//! - `nix-module check <files...>` - Check modules for errors without full eval
//! - `nix-module options <files...>` - List declared options with types/defaults
//!
//! # Example
//!
//! ```bash
//! # Evaluate modules
//! nix-module eval configuration.nix hardware.nix --format json
//!
//! # Check for errors
//! nix-module check ./config/*.nix --quiet
//!
//! # List options
//! nix-module options ./modules/ --format yaml
//! ```

mod commands;
pub mod nix_eval;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;
use tracing_subscriber::EnvFilter;

/// Exit codes for the CLI.
pub mod exit_codes {
    /// Success.
    pub const SUCCESS: u8 = 0;
    /// Evaluation error.
    pub const EVAL_ERROR: u8 = 1;
    /// Usage/argument error.
    pub const USAGE_ERROR: u8 = 2;
}

/// Output format for results.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    /// JSON format.
    #[default]
    Json,
    /// Nix expression format.
    Nix,
    /// YAML format.
    Yaml,
}

/// Nix module system CLI.
///
/// A fast, Rust-based Nix module evaluator with beautiful error messages.
#[derive(Parser)]
#[command(name = "nix-module")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Commands,

    /// Output format.
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Json, global = true)]
    format: OutputFormat,

    /// Output file (defaults to stdout).
    #[arg(long, short = 'o', global = true)]
    output: Option<PathBuf>,

    /// Strict mode: fail on any warning.
    #[arg(long, global = true)]
    strict: bool,

    /// Quiet mode: suppress non-error output (for CI).
    #[arg(long, short = 'q', global = true)]
    quiet: bool,

    /// Enable debug logging.
    #[arg(long, global = true)]
    debug: bool,
}

/// Available commands.
#[derive(Subcommand)]
pub enum Commands {
    /// Evaluate modules and output the resulting configuration.
    Eval {
        /// Input files or directories to evaluate.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Only output a specific attribute path.
        #[arg(long, short = 'A')]
        attr: Option<String>,

        /// Show raw Nix values (don't pretty-print).
        #[arg(long)]
        raw: bool,
    },

    /// Check modules for errors without full evaluation.
    Check {
        /// Input files or directories to check.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Exit with error on warnings (same as --strict).
        #[arg(long, short = 'W')]
        warnings_as_errors: bool,
    },

    /// List declared options with types and defaults.
    Options {
        /// Input files or directories to analyze.
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Filter options by prefix (e.g., "services.nginx").
        #[arg(long, short = 'p')]
        prefix: Option<String>,

        /// Include internal options (normally hidden).
        #[arg(long)]
        include_internal: bool,

        /// Show only option paths (no metadata).
        #[arg(long)]
        paths_only: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Initialize logging
    let filter = if cli.debug {
        EnvFilter::new("debug")
    } else if cli.quiet {
        EnvFilter::new("error")
    } else {
        EnvFilter::from_default_env().add_directive("warn".parse().unwrap())
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    let result = match cli.command {
        Commands::Eval { ref files, ref attr, raw } => {
            commands::eval::run(files, attr.as_deref(), raw, &cli)
        }
        Commands::Check { ref files, warnings_as_errors } => {
            commands::check::run(files, warnings_as_errors, &cli)
        }
        Commands::Options { ref files, ref prefix, include_internal, paths_only } => {
            commands::options::run(files, prefix.as_deref(), include_internal, paths_only, &cli)
        }
    };

    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            if !cli.quiet {
                eprintln!("error: {}", e);
            }
            ExitCode::from(exit_codes::EVAL_ERROR)
        }
    }
}
