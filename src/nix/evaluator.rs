//! Nix evaluator wrapper.
//!
//! This module provides a high-level interface for evaluating Nix expressions
//! using nix-bindings-rust.

use crate::nix::error::{NixError, NixResult};
use crate::types::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[cfg(feature = "nix-bindings")]
use crate::nix::error::TraceFrame;
#[cfg(feature = "nix-bindings")]
use std::path::PathBuf;
#[cfg(feature = "nix-bindings")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "nix-bindings")]
use crate::nix::convert::{nix_to_value, value_to_nix, LazyValue};
#[cfg(feature = "nix-bindings")]
use nix_bindings_expr::eval_state::{gc_register_my_thread, init, EvalState, ThreadRegistrationGuard};
#[cfg(feature = "nix-bindings")]
use nix_bindings_store::store::Store;

/// Global initialization flag.
#[cfg(feature = "nix-bindings")]
static INITIALIZED: AtomicBool = AtomicBool::new(false);

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

/// Initialize the Nix library.
///
/// This must be called once before creating any evaluators.
/// It's safe to call multiple times.
#[cfg(feature = "nix-bindings")]
pub fn initialize() -> NixResult<()> {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    init().map_err(|e| NixError::InitError {
        message: e.to_string(),
    })
}

/// Initialize the Nix library (no-op when feature not enabled).
#[cfg(not(feature = "nix-bindings"))]
pub fn initialize() -> NixResult<()> {
    Ok(())
}

/// High-level Nix evaluator.
///
/// This wraps the nix-bindings-rust types and provides a simpler interface
/// for evaluating Nix expressions.
#[cfg(feature = "nix-bindings")]
pub struct NixEvaluator {
    /// The evaluation state.
    eval_state: EvalState,
    /// Thread registration guard.
    _guard: ThreadRegistrationGuard,
    /// Configuration.
    config: NixConfig,
}

#[cfg(feature = "nix-bindings")]
impl NixEvaluator {
    /// Create a new evaluator with default configuration.
    pub fn new(config: NixConfig) -> NixResult<Self> {
        initialize()?;

        let guard = gc_register_my_thread().map_err(|e| NixError::ThreadRegistrationError {
            message: e.to_string(),
        })?;

        let store = Store::open(config.store_uri.as_deref(), config.store_params.clone())
            .map_err(|e| NixError::StoreError {
                message: e.to_string(),
            })?;

        let eval_state = EvalState::new(store, config.lookup_paths.clone()).map_err(|e| {
            NixError::InitError {
                message: e.to_string(),
            }
        })?;

        Ok(Self {
            eval_state,
            _guard: guard,
            config,
        })
    }

    /// Evaluate a Nix expression from a string.
    ///
    /// The `source_name` is used for error messages (e.g., "<repl>", "config.nix").
    pub fn evaluate_expr(&mut self, expr: &str) -> NixResult<Value> {
        self.evaluate_expr_with_source(expr, "<expr>")
    }

    /// Evaluate a Nix expression with a custom source name.
    pub fn evaluate_expr_with_source(
        &mut self,
        expr: &str,
        source_name: &str,
    ) -> NixResult<Value> {
        let nix_value = self
            .eval_state
            .eval_from_string(expr, source_name)
            .map_err(|e| parse_nix_error(e, source_name))?;

        nix_to_value(&mut self.eval_state, &nix_value)
    }

    /// Evaluate a Nix file.
    pub fn evaluate_file(&mut self, path: impl AsRef<Path>) -> NixResult<Value> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(NixError::FileNotFound {
                path: path.to_path_buf(),
            });
        }

        let contents = std::fs::read_to_string(path).map_err(|e| NixError::IoError {
            message: format!("failed to read {}: {}", path.display(), e),
        })?;

        let source_name = path.to_string_lossy();
        self.evaluate_expr_with_source(&contents, &source_name)
    }

    /// Evaluate a Nix expression and return a lazy value.
    ///
    /// This allows deferring full conversion until needed.
    pub fn evaluate_lazy(&mut self, expr: &str) -> NixResult<LazyValue> {
        let nix_value = self
            .eval_state
            .eval_from_string(expr, "<expr>")
            .map_err(|e| parse_nix_error(e, "<expr>"))?;

        Ok(LazyValue::new(nix_value))
    }

    /// Evaluate a Nix expression and extract an attribute.
    pub fn evaluate_attr(&mut self, expr: &str, attr: &str) -> NixResult<Value> {
        let full_expr = format!("({}).{}", expr, attr);
        self.evaluate_expr(&full_expr)
    }

    /// Call a Nix function with an argument.
    pub fn call_function(&mut self, func_expr: &str, arg: &Value) -> NixResult<Value> {
        let func_value = self
            .eval_state
            .eval_from_string(func_expr, "<function>")
            .map_err(|e| parse_nix_error(e, "<function>"))?;

        let arg_value = value_to_nix(&mut self.eval_state, arg)?;

        let result = self
            .eval_state
            .call(func_value, arg_value)
            .map_err(|e| NixError::evaluation(e.to_string()))?;

        nix_to_value(&mut self.eval_state, &result)
    }

    /// Import a Nix file and return its value.
    pub fn import(&mut self, path: impl AsRef<Path>) -> NixResult<Value> {
        let path = path.as_ref();
        let expr = format!("import {}", path.display());
        self.evaluate_expr(&expr)
    }

    /// Evaluate with builtins available in scope.
    pub fn evaluate_with_builtins(&mut self, expr: &str) -> NixResult<Value> {
        // Expression already has access to builtins
        self.evaluate_expr(expr)
    }

    /// Get the raw EvalState for advanced operations.
    pub fn eval_state(&mut self) -> &mut EvalState {
        &mut self.eval_state
    }

    /// Get the configuration.
    pub fn config(&self) -> &NixConfig {
        &self.config
    }
}

/// Parse a nix-bindings error into our error type.
#[cfg(feature = "nix-bindings")]
fn parse_nix_error(err: anyhow::Error, source: &str) -> NixError {
    let message = err.to_string();

    // Try to extract location information from the error message
    // Nix errors often look like: "error: ... at /path/to/file.nix:10:5:"
    if let Some(trace) = extract_trace(&message) {
        return NixError::evaluation_with_trace(message.clone(), trace);
    }

    // Check for common error patterns
    if message.contains("syntax error") || message.contains("unexpected") {
        return NixError::ParseError {
            file: PathBuf::from(source),
            message,
            line: None,
            column: None,
        };
    }

    if message.contains("undefined variable") {
        return NixError::evaluation(message);
    }

    if message.contains("attribute") && message.contains("missing") {
        // Try to extract attribute name
        return NixError::evaluation(message);
    }

    NixError::Other(err)
}

/// Extract trace frames from an error message.
#[cfg(feature = "nix-bindings")]
fn extract_trace(message: &str) -> Option<Vec<TraceFrame>> {
    let mut frames = Vec::new();

    for line in message.lines() {
        // Look for patterns like "at /path/file.nix:10:5"
        if let Some(at_pos) = line.find(" at ") {
            let location = &line[at_pos + 4..];
            if let Some((file, rest)) = location.rsplit_once(':') {
                if let Some((line_col, _)) = rest.split_once(':') {
                    if let Ok(line_num) = line_col.parse::<usize>() {
                        let desc = line[..at_pos].trim().to_string();
                        frames.push(
                            TraceFrame::new(desc)
                                .with_location(PathBuf::from(file), line_num, 1),
                        );
                    }
                }
            }
        }
    }

    if frames.is_empty() {
        None
    } else {
        Some(frames)
    }
}

// Fallback implementation when nix-bindings feature is not enabled

/// Nix evaluator (fallback without nix-bindings feature).
///
/// This is the fallback implementation when the nix-bindings feature is disabled.
/// All evaluation methods will return `NixError::NotInitialized` to indicate
/// that Nix evaluation is not available without the feature.
#[cfg(not(feature = "nix-bindings"))]
pub struct NixEvaluator {
    config: NixConfig,
}

#[cfg(not(feature = "nix-bindings"))]
impl NixEvaluator {
    /// Create a new evaluator (requires nix-bindings feature for actual evaluation).
    pub fn new(config: NixConfig) -> NixResult<Self> {
        Ok(Self { config })
    }

    /// Evaluate a Nix expression (requires nix-bindings feature).
    ///
    /// Returns `NixError::NotInitialized` when feature is not enabled.
    pub fn evaluate_expr(&mut self, _expr: &str) -> NixResult<Value> {
        Err(NixError::NotInitialized)
    }

    /// Evaluate a Nix file (requires nix-bindings feature).
    ///
    /// Returns `NixError::NotInitialized` when feature is not enabled.
    pub fn evaluate_file(&mut self, _path: impl AsRef<Path>) -> NixResult<Value> {
        Err(NixError::NotInitialized)
    }

    /// Get the configuration.
    pub fn config(&self) -> &NixConfig {
        &self.config
    }
}

/// Thread-safe evaluator handle.
///
/// This can be shared across threads, with each thread creating its own
/// evaluator instance when needed.
#[derive(Clone)]
pub struct EvaluatorHandle {
    config: Arc<NixConfig>,
}

impl EvaluatorHandle {
    /// Create a new evaluator handle.
    pub fn new(config: NixConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Create a thread-local evaluator.
    #[cfg(feature = "nix-bindings")]
    pub fn get(&self) -> NixResult<NixEvaluator> {
        NixEvaluator::new((*self.config).clone())
    }

    /// Create a thread-local evaluator (fallback version without nix-bindings).
    #[cfg(not(feature = "nix-bindings"))]
    pub fn get(&self) -> NixResult<NixEvaluator> {
        NixEvaluator::new((*self.config).clone())
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

    #[test]
    fn test_evaluator_handle() {
        let config = NixConfig::default();
        let handle = EvaluatorHandle::new(config);

        // Clone should work
        let _handle2 = handle.clone();
    }
}
