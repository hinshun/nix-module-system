//! High-level evaluation API for CLI tools and library consumers.
//!
//! This module provides an ergonomic, developer-friendly API for evaluating
//! Nix modules with the staged pipeline. It supports multiple input sources,
//! streaming diagnostics, and typed access to configuration values.
//!
//! # Quick Start
//!
//! ```ignore
//! use nix_module_system::api::{ModuleEvaluator, EvaluatedConfig};
//!
//! // Simple usage
//! let config = ModuleEvaluator::new()
//!     .add_file("./configuration.nix")?
//!     .add_file("./hardware.nix")?
//!     .evaluate()?;
//!
//! let nginx_port: i64 = config.get("services.nginx.port")?;
//! let enabled: bool = config.get("services.nginx.enable")?;
//! ```
//!
//! # With Options Introspection
//!
//! ```ignore
//! for option in config.options() {
//!     println!("{}: {} = {:?}", option.path, option.type_desc, option.default);
//! }
//! ```
//!
//! # With Error Streaming
//!
//! ```ignore
//! let config = ModuleEvaluator::new()
//!     .add_file("./config.nix")?
//!     .on_diagnostic(|diag| eprintln!("{}", diag))
//!     .evaluate()?;
//! ```

use crate::errors::{Diagnostic, EvalError, Severity};
use crate::eval::{collect_modules, filter_disabled, CollectedModule, EvalResult, OptionInfo, Pipeline};
use crate::parse::{self, Expr, Spanned};
use crate::types::{OptionPath, Value};
use indexmap::IndexMap;
use std::collections::HashSet;
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
        path: PathBuf,
        message: String,
    },
    /// Error parsing source code
    Parse {
        file: PathBuf,
        errors: Vec<String>,
    },
    /// Configuration value not found
    NotFound {
        path: String,
    },
    /// Type conversion error
    TypeMismatch {
        path: String,
        expected: &'static str,
        found: String,
    },
    /// Invalid path format
    InvalidPath {
        path: String,
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
            ApiError::Parse { file, errors } => {
                write!(f, "Parse errors in {}: {}", file.display(), errors.join(", "))
            }
            ApiError::NotFound { path } => {
                write!(f, "Configuration value not found: {}", path)
            }
            ApiError::TypeMismatch { path, expected, found } => {
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
        source: String,
        filename: PathBuf,
    },
    /// Module from a pre-parsed AST
    Ast {
        ast: Spanned<Expr>,
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

    /// Create a source from a pre-parsed AST
    pub fn ast(ast: Spanned<Expr>, filename: PathBuf) -> Self {
        ModuleSource::Ast { ast, filename }
    }

    /// Get the filename for this source
    pub fn filename(&self) -> &Path {
        match self {
            ModuleSource::File(p) => p,
            ModuleSource::String { filename, .. } => filename,
            ModuleSource::Ast { filename, .. } => filename,
        }
    }
}

// ============================================================================
// ModuleEvaluator Builder
// ============================================================================

/// Builder for configuring and running module evaluation.
///
/// Uses the builder pattern to configure evaluation options,
/// add module sources, and execute the evaluation pipeline.
///
/// # Example
///
/// ```ignore
/// let config = ModuleEvaluator::new()
///     .add_file("./configuration.nix")?
///     .add_string("{ config.my.option = true; }", "inline.nix")
///     .root_dir("./")
///     .on_diagnostic(|d| eprintln!("{}", d.message))
///     .evaluate()?;
/// ```
pub struct ModuleEvaluator {
    /// Module sources to evaluate
    sources: Vec<ModuleSource>,
    /// Disabled module keys
    disabled: HashSet<String>,
    /// Root directory for resolving relative paths
    root_dir: Option<PathBuf>,
    /// Diagnostic callback
    diagnostic_callback: Option<DiagnosticCallback>,
    /// Collected diagnostics (if no callback)
    diagnostics: Vec<Diagnostic>,
    /// Whether to continue on parse errors
    lenient_parsing: bool,
    /// Whether to include internal options in introspection
    include_internal: bool,
}

impl ModuleEvaluator {
    /// Create a new module evaluator with default settings.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            disabled: HashSet::new(),
            root_dir: None,
            diagnostic_callback: None,
            diagnostics: Vec::new(),
            lenient_parsing: false,
            include_internal: false,
        }
    }

    /// Add a module from a file path.
    ///
    /// The file will be read and parsed during evaluation.
    ///
    /// # Errors
    ///
    /// Returns an error if the path cannot be canonicalized.
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
    ///
    /// The source will be parsed with the given virtual filename.
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

    /// Add a module from a pre-parsed AST.
    ///
    /// Use this when you've already parsed the module and want to
    /// avoid re-parsing.
    pub fn add_ast(mut self, ast: Spanned<Expr>, filename: PathBuf) -> Self {
        self.sources.push(ModuleSource::Ast { ast, filename });
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
    ///
    /// Defaults to the current working directory.
    pub fn root_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.root_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Disable a specific module by key (usually its file path).
    pub fn disable_module<S: Into<String>>(mut self, key: S) -> Self {
        self.disabled.insert(key.into());
        self
    }

    /// Disable multiple modules.
    pub fn disable_modules<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for key in keys {
            self.disabled.insert(key.into());
        }
        self
    }

    /// Set a callback for receiving diagnostics during evaluation.
    ///
    /// The callback is invoked for each warning or error encountered.
    /// If no callback is set, diagnostics are collected and available
    /// via [`EvaluatedConfig::diagnostics()`].
    pub fn on_diagnostic<F>(mut self, callback: F) -> Self
    where
        F: Fn(&Diagnostic) + Send + Sync + 'static,
    {
        self.diagnostic_callback = Some(Arc::new(callback));
        self
    }

    /// Enable lenient parsing mode.
    ///
    /// In lenient mode, parse errors are recorded as diagnostics
    /// but evaluation continues with successfully parsed modules.
    pub fn lenient(mut self, lenient: bool) -> Self {
        self.lenient_parsing = lenient;
        self
    }

    /// Include internal options in introspection results.
    ///
    /// By default, options marked as `internal = true` are excluded
    /// from the options list.
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

    /// Process a module source into a CollectedModule.
    fn process_source(&mut self, source: ModuleSource) -> ApiResult<CollectedModule> {
        match source {
            ModuleSource::File(path) => {
                // File will be processed by collect_modules
                Ok(CollectedModule::new(path))
            }
            ModuleSource::String { source, filename } => {
                match parse::parse_module(&source, filename.clone()) {
                    Ok(ast) => Ok(CollectedModule::from_ast(filename, ast)),
                    Err(errors) => {
                        for err in &errors {
                            self.emit_diagnostic(Diagnostic::error(&err.message).with_code("E0100"));
                        }
                        if self.lenient_parsing {
                            Ok(CollectedModule::new(filename))
                        } else {
                            Err(ApiError::Parse {
                                file: filename,
                                errors: errors.into_iter().map(|e| e.message).collect(),
                            })
                        }
                    }
                }
            }
            ModuleSource::Ast { ast, filename } => Ok(CollectedModule::from_ast(filename, ast)),
        }
    }

    /// Execute the evaluation pipeline and return the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A file cannot be read
    /// - Parsing fails (unless in lenient mode)
    /// - Evaluation encounters a fatal error
    pub fn evaluate(mut self) -> ApiResult<EvaluatedConfig> {
        if self.sources.is_empty() {
            // Return empty config for no sources
            return Ok(EvaluatedConfig {
                result: EvalResult {
                    config: Value::Attrs(IndexMap::new()),
                    options: IndexMap::new(),
                    warnings: Vec::new(),
                },
                diagnostics: self.diagnostics,
                include_internal: self.include_internal,
            });
        }

        // Separate file sources from string/AST sources
        let mut file_paths = Vec::new();
        let mut inline_modules = Vec::new();

        // Take ownership of sources to avoid borrow issues
        let sources = std::mem::take(&mut self.sources);
        for source in sources {
            match source {
                ModuleSource::File(path) => {
                    file_paths.push(path);
                }
                other => {
                    let module = self.process_source(other)?;
                    inline_modules.push(module);
                }
            }
        }

        // Collect modules from file paths
        let mut collected = if !file_paths.is_empty() {
            collect_modules(file_paths, self.disabled.clone())?
        } else {
            Vec::new()
        };

        // Add inline modules
        collected.extend(inline_modules);

        // Filter disabled modules
        let active = filter_disabled(collected);

        // Run the evaluation pipeline
        let result = Pipeline::new().with_modules(active).run()?;

        // Convert warnings to diagnostics
        for warning in &result.warnings {
            self.emit_diagnostic(Diagnostic::warning(warning));
        }

        Ok(EvaluatedConfig {
            result,
            diagnostics: self.diagnostics,
            include_internal: self.include_internal,
        })
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
///
/// This struct wraps the evaluation result and provides convenient methods
/// for querying values with type conversion, introspecting options, and
/// accessing diagnostics.
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
    /// Get a typed configuration value at the given path.
    ///
    /// The path should be in dotted notation (e.g., "services.nginx.port").
    ///
    /// # Type Conversion
    ///
    /// This method supports automatic type conversion for:
    /// - `i64`, `i32`, etc. - from `Value::Int`
    /// - `f64`, `f32` - from `Value::Float`
    /// - `bool` - from `Value::Bool`
    /// - `String` - from `Value::String`
    /// - `PathBuf` - from `Value::Path`
    /// - `Vec<T>` - from `Value::List`
    /// - `Option<T>` - returns `None` if path not found
    ///
    /// # Example
    ///
    /// ```ignore
    /// let port: i64 = config.get("services.nginx.port")?;
    /// let enabled: bool = config.get("services.nginx.enable")?;
    /// let hosts: Vec<String> = config.get("services.nginx.hosts")?;
    /// ```
    pub fn get<T: FromValue>(&self, path: &str) -> ApiResult<T> {
        let option_path = OptionPath::from_dotted(path);
        let value = self.result.get(&option_path).ok_or_else(|| ApiError::NotFound {
            path: path.to_string(),
        })?;
        T::from_value(value, path)
    }

    /// Try to get a configuration value, returning None if not found.
    ///
    /// Unlike `get()`, this returns `None` for missing values instead of
    /// an error. Type conversion errors are still returned as errors.
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
    ///
    /// Returns an iterator of OptionInfo structs containing metadata
    /// about each option (path, type, default, description).
    ///
    /// By default, internal options are excluded. Use
    /// [`ModuleEvaluator::include_internal`] to include them.
    pub fn options(&self) -> impl Iterator<Item = &OptionInfo> {
        let include_internal = self.include_internal;
        self.result.options.values().filter(move |opt| {
            // Filter internal options unless include_internal is set
            include_internal || !opt.internal
        })
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
    ///
    /// Use this for advanced access to evaluation internals.
    pub fn into_result(self) -> EvalResult {
        self.result
    }
}

// ============================================================================
// FromValue Trait for Type Conversion
// ============================================================================

/// Trait for converting from Nix Value to Rust types.
///
/// Implement this trait for custom types to enable direct conversion
/// via `config.get::<YourType>("path")`.
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
// Convenience Functions
// ============================================================================

/// Evaluate modules from file paths with default settings.
///
/// This is a convenience function for simple use cases.
///
/// # Example
///
/// ```ignore
/// let config = evaluate_files(&["./configuration.nix"])?;
/// let enabled: bool = config.get("services.nginx.enable")?;
/// ```
pub fn evaluate_files<P: AsRef<Path>>(paths: &[P]) -> ApiResult<EvaluatedConfig> {
    let mut evaluator = ModuleEvaluator::new();
    for path in paths {
        evaluator = evaluator.add_file(path)?;
    }
    evaluator.evaluate()
}

/// Evaluate a single module from a string.
///
/// # Example
///
/// ```ignore
/// let source = r#"{ config = { services.nginx.enable = true; }; }"#;
/// let config = evaluate_string(source, "inline.nix")?;
/// ```
pub fn evaluate_string<S: Into<String>, P: AsRef<Path>>(
    source: S,
    filename: P,
) -> ApiResult<EvaluatedConfig> {
    ModuleEvaluator::new()
        .add_string(source, filename)
        .evaluate()
}

// ============================================================================
// OptionQuery for Introspection
// ============================================================================

/// Query builder for filtering and searching options.
///
/// # Example
///
/// ```ignore
/// let nginx_options: Vec<_> = config
///     .query_options()
///     .prefix("services.nginx")
///     .with_type("bool")
///     .collect();
/// ```
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
                // Check prefix
                if let Some(ref prefix) = self.prefix {
                    if !opt.path.to_dotted().starts_with(prefix) {
                        return false;
                    }
                }
                // Check type
                if let Some(ref type_filter) = self.type_filter {
                    if !opt.type_desc.contains(type_filter) {
                        return false;
                    }
                }
                // Check default
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
    pub fn query_options(&self) -> OptionQuery {
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
        assert!(evaluator.disabled.is_empty());
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
        // This test just ensures OptionQuery compiles correctly
        let config = ModuleEvaluator::new().evaluate().unwrap();
        let _options: Vec<_> = config.query_options().prefix("services").collect();
    }
}
