//! # Nix Module System
//!
//! A high-performance Nix module system with Rust primops for:
//! - Type checking and merging (via Nix builtins plugin)
//! - Conditional/priority processing
//! - Beautiful error messages via ariadne
//!
//! ## Architecture
//!
//! The Nix evaluator is in charge. This crate provides:
//! - Core type system (`types/`) — type checking and merge logic
//! - Merge engine (`merge/`) — conditional, priority, and strategy handling
//! - Evaluation types (`eval/`) — result and option metadata types
//! - Error reporting (`errors/`) — unified error types with ariadne
//! - Nix integration (`nix/`) — value conversion and error bridging
//!
//! The `nix-module-plugin` crate wraps this as a Nix builtins plugin (cdylib).

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod api;
pub mod docs;
pub mod errors;
pub mod eval;
pub mod merge;
pub mod nix;
pub mod types;

#[cfg(feature = "lsp")]
pub mod lsp;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
