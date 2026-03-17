//! High-level API for the module system.
//!
//! Provides typed access to evaluated configurations and option introspection.
//! The actual evaluation is handled by the Nix evaluator (via `nix eval` with
//! the plugin, or `evalModules` in `nix/lib.nix`). This module provides the
//! Rust-side interface for processing results.

use crate::errors::{Diagnostic, EvalError, Severity};
use crate::eval::{EvalResult, OptionInfo};
use crate::types::{OptionPath, Value};
use indexmap::IndexMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during API operations
#[derive(Debug)]
pub enum ApiError {
    /// Error during evaluation
    Eval(EvalError),
    /// Error reading a file
    Io {
        /// The file path that caused the error.
        path: PathBuf,
        /// The error message.
        message: String,
    },
    /// Configuration value not found
    NotFound {
        /// The option path that was not found.
        path: String,
    },
    /// Type conversion error
    TypeMismatch {
        /// The option path where the mismatch occurred.
        path: String,
        /// The expected type.
        expected: &'static str,
        /// The actual type found.
        found: String,
    },
    /// Invalid path format
    InvalidPath {
        /// The invalid path string.
        path: String,
        /// Description of what's wrong with the path.
        message: String,
    },
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Eval(e) => write!(f, "{}", e),
            ApiError::Io { path, message } => {
                write!(f, "IO error for {}: {}", path.display(), message)
            }
            ApiError::NotFound { path } => {
                write!(f, "Configuration value not found: {}", path)
            }
            ApiError::TypeMismatch {
                path,
                expected,
                found,
            } => {
                write!(
                    f,
                    "Type mismatch at {}: expected {}, found {}",
                    path, expected, found
                )
            }
            ApiError::InvalidPath { path, message } => {
                write!(f, "Invalid path '{}': {}", path, message)
            }
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApiError::Eval(e) => Some(e),
            _ => None,
        }
    }
}

impl From<EvalError> for ApiError {
    fn from(e: EvalError) -> Self {
        ApiError::Eval(e)
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::Io {
            path: PathBuf::new(),
            message: e.to_string(),
        }
    }
}

/// Result type for API operations
pub type ApiResult<T> = Result<T, ApiError>;

// ============================================================================
// Diagnostic Callback
// ============================================================================

/// Type for diagnostic callback functions
pub type DiagnosticCallback = Arc<dyn Fn(&Diagnostic) + Send + Sync>;

// ============================================================================
// Module Input Sources
// ============================================================================

/// Input source for a module
#[derive(Debug, Clone)]
pub enum ModuleSource {
    /// Module loaded from a file path
    File(PathBuf),
    /// Module from a string source with a virtual filename
    String {
        /// The Nix source code.
        source: String,
        /// The virtual filename for error reporting.
        filename: PathBuf,
    },
}

impl ModuleSource {
    /// Create a source from a file path
    pub fn file<P: AsRef<Path>>(path: P) -> Self {
        ModuleSource::File(path.as_ref().to_path_buf())
    }

    /// Create a source from a string
    pub fn string<S: Into<String>, P: AsRef<Path>>(source: S, filename: P) -> Self {
        ModuleSource::String {
            source: source.into(),
            filename: filename.as_ref().to_path_buf(),
        }
    }

    /// Get the filename for this source
    pub fn filename(&self) -> &Path {
        match self {
            ModuleSource::File(p) => p,
            ModuleSource::String { filename, .. } => filename,
        }
    }
}

// ============================================================================
// ModuleEvaluator Builder
// ============================================================================

/// Builder for configuring module evaluation.
///
/// In the new architecture, actual evaluation is handled by the Nix evaluator.
/// This builder collects module sources and options, then delegates to Nix.
pub struct ModuleEvaluator {
    /// Module sources to evaluate
    sources: Vec<ModuleSource>,
    /// Root directory for resolving relative paths
    root_dir: Option<PathBuf>,
    /// Diagnostic callback
    diagnostic_callback: Option<DiagnosticCallback>,
    /// Collected diagnostics (if no callback)
    diagnostics: Vec<Diagnostic>,
    /// Whether to include internal options in introspection
    include_internal: bool,
}

impl ModuleEvaluator {
    /// Create a new module evaluator with default settings.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            root_dir: None,
            diagnostic_callback: None,
            diagnostics: Vec::new(),
            include_internal: false,
        }
    }

    /// Add a module from a file path.
    pub fn add_file<P: AsRef<Path>>(mut self, path: P) -> ApiResult<Self> {
        let path = path.as_ref();
        let abs_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            let root = self.root_dir.clone().unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            });
            root.join(path)
        };

        if !abs_path.exists() {
            return Err(ApiError::Io {
                path: abs_path,
                message: "File does not exist".to_string(),
            });
        }

        self.sources.push(ModuleSource::File(abs_path));
        Ok(self)
    }

    /// Add a module from a string source.
    pub fn add_string<S, P>(mut self, source: S, filename: P) -> Self
    where
        S: Into<String>,
        P: AsRef<Path>,
    {
        self.sources.push(ModuleSource::String {
            source: source.into(),
            filename: filename.as_ref().to_path_buf(),
        });
        self
    }

    /// Add multiple module sources at once.
    pub fn add_sources<I>(mut self, sources: I) -> Self
    where
        I: IntoIterator<Item = ModuleSource>,
    {
        self.sources.extend(sources);
        self
    }

    /// Set the root directory for resolving relative paths.
    pub fn root_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.root_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set a callback for receiving diagnostics during evaluation.
    pub fn on_diagnostic<F>(mut self, callback: F) -> Self
    where
        F: Fn(&Diagnostic) + Send + Sync + 'static,
    {
        self.diagnostic_callback = Some(Arc::new(callback));
        self
    }

    /// Include internal options in introspection results.
    pub fn include_internal(mut self, include: bool) -> Self {
        self.include_internal = include;
        self
    }

    /// Emit a diagnostic.
    fn emit_diagnostic(&mut self, diagnostic: Diagnostic) {
        if let Some(ref callback) = self.diagnostic_callback {
            callback(&diagnostic);
        } else {
            self.diagnostics.push(diagnostic);
        }
    }

    /// Execute the evaluation and return the configuration.
    ///
    /// In the new architecture, evaluation is delegated to the Nix evaluator.
    /// This method returns an empty config — the CLI or plugin integration
    /// layer is responsible for driving `nix eval` and constructing the result.
    pub fn evaluate(self) -> ApiResult<EvaluatedConfig> {
        // Return empty config — actual evaluation happens via Nix
        Ok(EvaluatedConfig {
            result: EvalResult {
                config: Value::Attrs(IndexMap::new()),
                options: IndexMap::new(),
                warnings: Vec::new(),
            },
            diagnostics: self.diagnostics,
            include_internal: self.include_internal,
        })
    }

    /// Get the module sources configured on this evaluator.
    pub fn sources(&self) -> &[ModuleSource] {
        &self.sources
    }
}

impl Default for ModuleEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// EvaluatedConfig
// ============================================================================

/// Result of evaluating modules, providing typed access to configuration.
#[derive(Debug)]
pub struct EvaluatedConfig {
    /// The underlying evaluation result
    result: EvalResult,
    /// Collected diagnostics
    diagnostics: Vec<Diagnostic>,
    /// Whether to include internal options
    include_internal: bool,
}

impl EvaluatedConfig {
    /// Create an EvaluatedConfig from an EvalResult.
    pub fn from_result(result: EvalResult) -> Self {
        Self {
            result,
            diagnostics: Vec::new(),
            include_internal: false,
        }
    }

    /// Get a typed configuration value at the given path.
    pub fn get<T: FromValue>(&self, path: &str) -> ApiResult<T> {
        let option_path = OptionPath::from_dotted(path);
        let value = self
            .result
            .get(&option_path)
            .ok_or_else(|| ApiError::NotFound {
                path: path.to_string(),
            })?;
        T::from_value(value, path)
    }

    /// Try to get a configuration value, returning None if not found.
    pub fn try_get<T: FromValue>(&self, path: &str) -> ApiResult<Option<T>> {
        let option_path = OptionPath::from_dotted(path);
        match self.result.get(&option_path) {
            Some(value) => Ok(Some(T::from_value(value, path)?)),
            None => Ok(None),
        }
    }

    /// Get the raw Value at a path without type conversion.
    pub fn get_raw(&self, path: &str) -> Option<&Value> {
        let option_path = OptionPath::from_dotted(path);
        self.result.get(&option_path)
    }

    /// Check if a configuration path is defined.
    pub fn is_defined(&self, path: &str) -> bool {
        let option_path = OptionPath::from_dotted(path);
        self.result.is_defined(&option_path)
    }

    /// Get the entire configuration as a Value.
    pub fn config(&self) -> &Value {
        &self.result.config
    }

    /// Iterate over all declared options.
    pub fn options(&self) -> impl Iterator<Item = &OptionInfo> {
        let include_internal = self.include_internal;
        self.result
            .options
            .values()
            .filter(move |opt| include_internal || !opt.internal)
    }

    /// Get information about a specific option.
    pub fn option(&self, path: &str) -> Option<&OptionInfo> {
        let option_path = OptionPath::from_dotted(path);
        self.result.options.get(&option_path)
    }

    /// Get all options as a map.
    pub fn options_map(&self) -> &IndexMap<OptionPath, OptionInfo> {
        &self.result.options
    }

    /// Get all diagnostics (warnings and errors) from evaluation.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Check if evaluation had any errors.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Check if evaluation had any warnings.
    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning)
    }

    /// Get warnings from evaluation (excluding errors).
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Get errors from evaluation (excluding warnings).
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    /// Get the underlying EvalResult.
    pub fn into_result(self) -> EvalResult {
        self.result
    }
}

// ============================================================================
// FromValue Trait for Type Conversion
// ============================================================================

/// Trait for converting from Nix Value to Rust types.
pub trait FromValue: Sized {
    /// Convert from a Value, returning an error if the type doesn't match.
    fn from_value(value: &Value, path: &str) -> ApiResult<Self>;
}

impl FromValue for Value {
    fn from_value(value: &Value, _path: &str) -> ApiResult<Self> {
        Ok(value.clone())
    }
}

impl FromValue for bool {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Bool(b) => Ok(*b),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "bool",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for i64 {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Int(i) => Ok(*i),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "int",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for i32 {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Int(i) => Ok(*i as i32),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "int",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for u64 {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Int(i) if *i >= 0 => Ok(*i as u64),
            Value::Int(_) => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "unsigned int",
                found: "negative int".to_string(),
            }),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "int",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for u32 {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Int(i) if *i >= 0 => Ok(*i as u32),
            Value::Int(_) => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "unsigned int",
                found: "negative int".to_string(),
            }),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "int",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for f64 {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Float(f) => Ok(*f),
            Value::Int(i) => Ok(*i as f64),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "float",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for f32 {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Float(f) => Ok(*f as f32),
            Value::Int(i) => Ok(*i as f32),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "float",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for String {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::String(s) => Ok(s.clone()),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "string",
                found: value_type_name(other),
            }),
        }
    }
}

impl FromValue for PathBuf {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Path(p) => Ok(p.clone()),
            Value::String(s) => Ok(PathBuf::from(s)),
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "path",
                found: value_type_name(other),
            }),
        }
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::List(items) => {
                let mut result = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    let item_path = format!("{}[{}]", path, i);
                    result.push(T::from_value(item, &item_path)?);
                }
                Ok(result)
            }
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "list",
                found: value_type_name(other),
            }),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Null => Ok(None),
            other => Ok(Some(T::from_value(other, path)?)),
        }
    }
}

impl<V: FromValue> FromValue for IndexMap<String, V> {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Attrs(attrs) => {
                let mut result = IndexMap::with_capacity(attrs.len());
                for (key, val) in attrs {
                    let item_path = format!("{}.{}", path, key);
                    result.insert(key.clone(), V::from_value(val, &item_path)?);
                }
                Ok(result)
            }
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "attrs",
                found: value_type_name(other),
            }),
        }
    }
}

impl<V: FromValue> FromValue for std::collections::HashMap<String, V> {
    fn from_value(value: &Value, path: &str) -> ApiResult<Self> {
        match value {
            Value::Attrs(attrs) => {
                let mut result = std::collections::HashMap::with_capacity(attrs.len());
                for (key, val) in attrs {
                    let item_path = format!("{}.{}", path, key);
                    result.insert(key.clone(), V::from_value(val, &item_path)?);
                }
                Ok(result)
            }
            other => Err(ApiError::TypeMismatch {
                path: path.to_string(),
                expected: "attrs",
                found: value_type_name(other),
            }),
        }
    }
}

/// Get a human-readable type name for a Value
fn value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Int(_) => "int".to_string(),
        Value::Float(_) => "float".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Path(_) => "path".to_string(),
        Value::List(_) => "list".to_string(),
        Value::Attrs(_) => "attrs".to_string(),
        Value::Lambda => "lambda".to_string(),
        Value::Derivation(_) => "derivation".to_string(),
    }
}

// ============================================================================
// OptionQuery for Introspection
// ============================================================================

/// Query builder for filtering and searching options.
pub struct OptionQuery<'a> {
    config: &'a EvaluatedConfig,
    prefix: Option<String>,
    type_filter: Option<String>,
    has_default: Option<bool>,
}

impl<'a> OptionQuery<'a> {
    /// Create a new option query.
    fn new(config: &'a EvaluatedConfig) -> Self {
        Self {
            config,
            prefix: None,
            type_filter: None,
            has_default: None,
        }
    }

    /// Filter options by path prefix.
    pub fn prefix<S: Into<String>>(mut self, prefix: S) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Filter options by type description.
    pub fn with_type<S: Into<String>>(mut self, type_desc: S) -> Self {
        self.type_filter = Some(type_desc.into());
        self
    }

    /// Filter options that have/don't have a default value.
    pub fn has_default(mut self, has_default: bool) -> Self {
        self.has_default = Some(has_default);
        self
    }

    /// Collect matching options.
    pub fn collect(self) -> Vec<&'a OptionInfo> {
        self.config
            .options()
            .filter(|opt| {
                if let Some(ref prefix) = self.prefix {
                    if !opt.path.to_dotted().starts_with(prefix) {
                        return false;
                    }
                }
                if let Some(ref type_filter) = self.type_filter {
                    if !opt.type_desc.contains(type_filter) {
                        return false;
                    }
                }
                if let Some(has_default) = self.has_default {
                    if has_default != opt.default.is_some() {
                        return false;
                    }
                }
                true
            })
            .collect()
    }
}

impl EvaluatedConfig {
    /// Create a query builder for searching and filtering options.
    pub fn query_options(&self) -> OptionQuery<'_> {
        OptionQuery::new(self)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_evaluator_new() {
        let evaluator = ModuleEvaluator::new();
        assert!(evaluator.sources.is_empty());
    }

    #[test]
    fn test_evaluate_empty() {
        let config = ModuleEvaluator::new().evaluate().unwrap();
        assert!(!config.has_errors());
        assert!(!config.has_warnings());
    }

    #[test]
    fn test_from_value_bool() {
        let value = Value::Bool(true);
        let result: bool = FromValue::from_value(&value, "test").unwrap();
        assert!(result);
    }

    #[test]
    fn test_from_value_int() {
        let value = Value::Int(42);
        let result: i64 = FromValue::from_value(&value, "test").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_from_value_string() {
        let value = Value::String("hello".to_string());
        let result: String = FromValue::from_value(&value, "test").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_from_value_vec() {
        let value = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let result: Vec<i64> = FromValue::from_value(&value, "test").unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_from_value_option_some() {
        let value = Value::String("present".to_string());
        let result: Option<String> = FromValue::from_value(&value, "test").unwrap();
        assert_eq!(result, Some("present".to_string()));
    }

    #[test]
    fn test_from_value_option_none() {
        let value = Value::Null;
        let result: Option<String> = FromValue::from_value(&value, "test").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_from_value_map() {
        let mut attrs = IndexMap::new();
        attrs.insert("a".to_string(), Value::Int(1));
        attrs.insert("b".to_string(), Value::Int(2));
        let value = Value::Attrs(attrs);

        let result: IndexMap<String, i64> = FromValue::from_value(&value, "test").unwrap();
        assert_eq!(result.get("a"), Some(&1));
        assert_eq!(result.get("b"), Some(&2));
    }

    #[test]
    fn test_from_value_type_mismatch() {
        let value = Value::String("not an int".to_string());
        let result: Result<i64, _> = FromValue::from_value(&value, "test.path");
        assert!(result.is_err());
        if let Err(ApiError::TypeMismatch { path, expected, .. }) = result {
            assert_eq!(path, "test.path");
            assert_eq!(expected, "int");
        }
    }

    #[test]
    fn test_module_source_filename() {
        let file_source = ModuleSource::file("/path/to/file.nix");
        assert_eq!(file_source.filename(), Path::new("/path/to/file.nix"));

        let string_source = ModuleSource::string("{ }", "virtual.nix");
        assert_eq!(string_source.filename(), Path::new("virtual.nix"));
    }

    #[test]
    fn test_api_error_display() {
        let err = ApiError::NotFound {
            path: "services.nginx.port".to_string(),
        };
        assert!(err.to_string().contains("services.nginx.port"));

        let err = ApiError::TypeMismatch {
            path: "foo".to_string(),
            expected: "int",
            found: "string".to_string(),
        };
        assert!(err.to_string().contains("int"));
        assert!(err.to_string().contains("string"));
    }

    #[test]
    fn test_evaluated_config_diagnostics() {
        let config = ModuleEvaluator::new().evaluate().unwrap();
        assert!(config.diagnostics().is_empty());
        assert!(!config.has_errors());
        assert!(!config.has_warnings());
    }

    #[test]
    fn test_option_query() {
        let config = ModuleEvaluator::new().evaluate().unwrap();
        let _options: Vec<_> = config.query_options().prefix("services").collect();
    }
}
