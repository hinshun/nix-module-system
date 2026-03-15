//! Nix evaluation integration using nix-bindings-rust.
//!
//! This module provides a bridge to evaluate Nix expressions and convert
//! between Nix values and our internal `Value` type.
//!
//! # Example
//!
//! ```ignore
//! use nix_module_system::nix::{NixEvaluator, NixConfig};
//!
//! let config = NixConfig::default();
//! let evaluator = NixEvaluator::new(config)?;
//!
//! let value = evaluator.evaluate_expr("{ x = 1; y = 2; }")?;
//! println!("{:?}", value);
//! ```

mod convert;
mod error;
mod evaluator;

pub use convert::*;
pub use error::*;
pub use evaluator::*;

#[cfg(test)]
mod tests;
