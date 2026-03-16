//! # Nix Module System v2
//!
//! A high-performance Nix module system implementation with:
//! - Rust-based type checking and merging
//! - No fixed-point evaluation
//! - Beautiful error messages via ariadne
//! - LSP support
//!
//! ## Architecture
//!
//! This crate provides the core module system logic. The evaluation uses a
//! staged pipeline instead of fixed-point iteration:
//!
//! 1. **Parse**: Extract module structure (imports, options, config)
//! 2. **Collect**: Resolve imports and build dependency graph
//! 3. **Declare**: Process option declarations (no config access needed)
//! 4. **Define**: Merge config definitions using lattice unification
//!
//! ## Usage
//!
//! ```ignore
//! use nix_module_system::api::{ModuleEvaluator, EvaluatedConfig};
//!
//! let config = ModuleEvaluator::new()
//!     .add_file("./configuration.nix")?
//!     .add_file("./hardware.nix")?
//!     .evaluate()?;
//!
//! let nginx_port: i64 = config.get("services.nginx.port")?;
//! let enabled: bool = config.get("services.nginx.enable")?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod api;
pub mod docs;
pub mod errors;
pub mod eval;
pub mod merge;
pub mod nix;
pub mod parse;
pub mod types;

#[cfg(feature = "lsp")]
pub mod lsp;

/// Version of the module system
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
