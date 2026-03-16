//! Nix evaluator configuration.
//!
//! This module provides configuration types for Nix evaluation.
//! Actual evaluation backends are provided by downstream crates
//! (e.g., `nix-module-cli` uses `nix-bindings-rust`).

use std::collections::HashMap;

/// Configuration for the Nix evaluator.
#[derive(Debug, Clone, Default)]
pub struct NixConfig {
    /// Store URI (None for default store).
    pub store_uri: Option<String>,
    /// Additional store parameters.
    pub store_params: HashMap<String, String>,
    /// Lookup paths (like NIX_PATH).
    pub lookup_paths: Vec<String>,
    /// Whether to allow impure evaluation.
    pub allow_impure: bool,
    /// Whether to enable tracing for debugging.
    pub enable_trace: bool,
    /// Maximum evaluation depth (0 for unlimited).
    pub max_depth: usize,
}

impl NixConfig {
    /// Create a new config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the store URI.
    pub fn with_store(mut self, uri: impl Into<String>) -> Self {
        self.store_uri = Some(uri.into());
        self
    }

    /// Add a lookup path.
    pub fn with_lookup_path(mut self, path: impl Into<String>) -> Self {
        self.lookup_paths.push(path.into());
        self
    }

    /// Enable impure evaluation.
    pub fn allow_impure(mut self) -> Self {
        self.allow_impure = true;
        self
    }

    /// Enable debug tracing.
    pub fn with_trace(mut self) -> Self {
        self.enable_trace = true;
        self
    }
}

#[cfg(test)]
mod evaluator_tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = NixConfig::new()
            .with_lookup_path("nixpkgs=/nix/store/abc")
            .allow_impure()
            .with_trace();

        assert!(config.allow_impure);
        assert!(config.enable_trace);
        assert_eq!(config.lookup_paths.len(), 1);
    }
}
