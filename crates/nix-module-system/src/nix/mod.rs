//! Nix evaluation support.
//!
//! This module provides:
//! - Error types for Nix evaluation (`NixError`, `NixResult`)
//! - JSON/Value conversion helpers
//! - Evaluator configuration (`NixConfig`)

mod convert;
/// Nix error types.
pub mod error;
mod evaluator;

pub use convert::*;
pub use error::*;
pub use evaluator::*;

#[cfg(test)]
mod tests;
