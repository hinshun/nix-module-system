//! Language Server Protocol implementation.
//!
//! This module provides LSP support for the Nix module system,
//! including completions, hover, and diagnostics.

mod registry;

#[cfg(feature = "lsp")]
mod completion;
#[cfg(feature = "lsp")]
mod hover;
#[cfg(feature = "lsp")]
mod server;

pub use registry::OptionRegistry;

#[cfg(feature = "lsp")]
pub use completion::*;
#[cfg(feature = "lsp")]
pub use hover::*;
#[cfg(feature = "lsp")]
pub use server::*;

use crate::types::OptionPath;

/// Information about a module option for LSP features
#[derive(Debug, Clone)]
pub struct OptionCompletion {
    /// Option name (last component of path)
    pub name: String,
    /// Full option path
    pub path: OptionPath,
    /// Type description
    pub type_desc: String,
    /// Option description
    pub description: Option<String>,
    /// Default value as string
    pub default: Option<String>,
}

/// LSP capability flags
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    /// Support for option completions
    pub completion: bool,
    /// Support for hover information
    pub hover: bool,
    /// Support for go-to-definition
    pub definition: bool,
    /// Support for find references
    pub references: bool,
    /// Support for diagnostics
    pub diagnostics: bool,
}

impl Capabilities {
    /// All capabilities enabled
    pub fn all() -> Self {
        Self {
            completion: true,
            hover: true,
            definition: true,
            references: true,
            diagnostics: true,
        }
    }

    /// No capabilities enabled
    pub fn none() -> Self {
        Self::default()
    }

    /// Only completion enabled
    pub fn completion_only() -> Self {
        Self {
            completion: true,
            ..Default::default()
        }
    }

    /// Only hover enabled
    pub fn hover_only() -> Self {
        Self {
            hover: true,
            ..Default::default()
        }
    }
}
