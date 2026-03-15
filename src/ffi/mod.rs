//! FFI interface for Nix plugin integration.
//!
//! This module handles the C/C++ FFI boundary for integrating
//! with the Nix evaluator as a plugin.

// Re-export main FFI types from lib.rs
pub use crate::{EvalState, NixContext, NixValue};

/// FFI status codes
pub mod status {
    /// Operation succeeded
    pub const OK: i32 = 0;
    /// Generic error
    pub const ERROR: i32 = -1;
    /// Type error
    pub const TYPE_ERROR: i32 = -2;
    /// Memory allocation error
    pub const ALLOC_ERROR: i32 = -3;
}
