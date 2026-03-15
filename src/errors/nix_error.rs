//! Unified error type for all Nix error sources.
//!
//! This module provides `NixError`, a comprehensive error type that can
//! represent errors from all sources:
//! - Nix syntax errors (from parser)
//! - Nix semantic/evaluation errors (undefined variables, type errors)
//! - Module system errors (type mismatches, undefined options, merge conflicts)
//! - External errors (FFI, IO, etc.)

use super::{
    DiagnosticReport, ErrorCode, Label, LabelStyle, RelatedLocation, Severity, SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Unified error type covering all error sources in the Nix module system.
///
/// This is the top-level error type that should be used in public APIs.
/// It can represent any error that can occur during parsing, evaluation,
/// or module system operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum NixError {
    // ========== Syntax Errors ==========
    /// Unexpected token during parsing
    UnexpectedToken {
        /// What was expected
        expected: Vec<String>,
        /// What was found
        found: String,
        /// Location of the error
        span: SourceSpan,
    },

    /// Unexpected end of file
    UnexpectedEof {
        /// What was expected
        expected: Vec<String>,
        /// Location of the error
        span: SourceSpan,
    },

    /// Invalid escape sequence in string
    InvalidEscape {
        /// The invalid escape sequence
        sequence: String,
        /// Location of the error
        span: SourceSpan,
    },

    /// Unterminated string literal
    UnterminatedString {
        /// Location where the string started
        start_span: SourceSpan,
    },

    /// Invalid number literal
    InvalidNumber {
        /// The invalid literal
        literal: String,
        /// Location of the error
        span: SourceSpan,
    },

    /// Generic parse error
    ParseError {
        /// Error message
        message: String,
        /// Location of the error
        span: SourceSpan,
        /// Recovery hints
        hints: Vec<String>,
    },

    // ========== Semantic Errors ==========
    /// Undefined variable reference
    UndefinedVariable {
        /// Variable name
        name: String,
        /// Location of the reference
        span: SourceSpan,
        /// Similar names that might have been intended
        similar: Vec<String>,
    },

    /// Type mismatch in expression
    TypeMismatch {
        /// Expected type
        expected: String,
        /// Actual type
        found: String,
        /// Location of the error
        span: SourceSpan,
        /// Context (e.g., "in function argument")
        context: Option<String>,
    },

    /// Assertion failure
    AssertionFailed {
        /// Assertion message (if any)
        message: Option<String>,
        /// Location of the assertion
        span: SourceSpan,
    },

    /// Throw expression
    Throw {
        /// Throw message
        message: String,
        /// Location of the throw
        span: SourceSpan,
    },

    /// Abort expression
    Abort {
        /// Abort message
        message: String,
        /// Location of the abort
        span: SourceSpan,
    },

    /// Attribute not found
    AttributeNotFound {
        /// Attribute name
        name: String,
        /// Attribute set type/description
        attrset_type: Option<String>,
        /// Location of the access
        span: SourceSpan,
        /// Available attributes
        available: Vec<String>,
    },

    /// Division by zero
    DivisionByZero {
        /// Location of the division
        span: SourceSpan,
    },

    /// Index out of bounds
    IndexOutOfBounds {
        /// The index used
        index: i64,
        /// The list length
        length: usize,
        /// Location of the access
        span: SourceSpan,
    },

    /// Cannot coerce value
    CoercionError {
        /// Source type
        from: String,
        /// Target type
        to: String,
        /// Location of the coercion
        span: SourceSpan,
    },

    /// Function arity mismatch
    ArityMismatch {
        /// Expected number of arguments
        expected: usize,
        /// Actual number of arguments
        found: usize,
        /// Location of the call
        span: SourceSpan,
    },

    // ========== Module System Errors ==========
    /// Undefined option in module
    UndefinedOption {
        /// Option path
        path: String,
        /// Location of the reference
        span: Option<SourceSpan>,
        /// Available options
        available: Vec<String>,
    },

    /// Missing option definition
    MissingDefinition {
        /// Option path
        path: String,
        /// Location of the option declaration
        declaration_span: Option<SourceSpan>,
    },

    /// Conflicting option definitions
    ConflictingDefinitions {
        /// Option path
        path: String,
        /// Locations of conflicting definitions
        definitions: Vec<RelatedLocation>,
        /// Values in conflict (as strings)
        values: Vec<String>,
    },

    /// Invalid option type
    InvalidOptionType {
        /// Option path
        path: String,
        /// Expected type
        expected: String,
        /// Actual type
        found: String,
        /// Location of the definition
        span: Option<SourceSpan>,
    },

    /// Read-only option modified
    ReadOnlyViolation {
        /// Option path
        path: String,
        /// Location of the modification attempt
        span: Option<SourceSpan>,
        /// Location of the read-only declaration
        declaration_span: Option<SourceSpan>,
    },

    /// Circular module dependency
    CircularDependency {
        /// The cycle as a list of module paths
        cycle: Vec<PathBuf>,
    },

    /// Module not found
    ModuleNotFound {
        /// Path being imported
        path: PathBuf,
        /// Search paths that were checked
        search_paths: Vec<PathBuf>,
        /// Location of the import
        import_span: Option<SourceSpan>,
    },

    /// Invalid module structure
    InvalidModule {
        /// Module path
        path: PathBuf,
        /// Description of the issue
        message: String,
        /// Location of the issue
        span: Option<SourceSpan>,
    },

    /// Module class mismatch
    ModuleClassMismatch {
        /// Expected class
        expected: String,
        /// Found class
        found: String,
        /// Module path
        path: Option<PathBuf>,
    },

    /// Infinite recursion in evaluation
    InfiniteRecursion {
        /// Path where recursion was detected
        path: Option<String>,
        /// The evaluation cycle
        cycle: Vec<String>,
    },

    // ========== External Errors ==========
    /// IO error
    IoError {
        /// Path involved (if any)
        path: Option<PathBuf>,
        /// Error message
        message: String,
    },

    /// FFI error from external Nix
    FfiError {
        /// Error message from external source
        message: String,
        /// External error code (if any)
        external_code: Option<String>,
    },

    /// Evaluation timeout
    Timeout {
        /// Timeout in seconds
        seconds: u64,
        /// What was being evaluated
        context: Option<String>,
    },

    // ========== Internal Errors ==========
    /// Internal error (bug)
    InternalError {
        /// Error message
        message: String,
        /// Location (if available)
        span: Option<SourceSpan>,
    },

    /// Feature not implemented
    NotImplemented {
        /// Feature description
        feature: String,
        /// Location (if available)
        span: Option<SourceSpan>,
    },

    /// Multiple errors bundled together
    Multiple {
        /// The errors
        errors: Vec<NixError>,
    },
}

impl NixError {
    /// Get the error code for this error.
    pub fn code(&self) -> ErrorCode {
        match self {
            // Syntax errors
            NixError::UnexpectedToken { .. } => ErrorCode::E0300,
            NixError::UnexpectedEof { .. } => ErrorCode::E0301,
            NixError::InvalidEscape { .. } => ErrorCode::E0302,
            NixError::UnterminatedString { .. } => ErrorCode::E0303,
            NixError::InvalidNumber { .. } => ErrorCode::E0304,
            NixError::ParseError { .. } => ErrorCode::E0300,

            // Semantic errors
            NixError::UndefinedVariable { .. } => ErrorCode::E0400,
            NixError::TypeMismatch { .. } => ErrorCode::E0001,
            NixError::AssertionFailed { .. } => ErrorCode::E0404,
            NixError::Throw { .. } => ErrorCode::E0405,
            NixError::Abort { .. } => ErrorCode::E0406,
            NixError::AttributeNotFound { .. } => ErrorCode::E0109,
            NixError::DivisionByZero { .. } => ErrorCode::E0107,
            NixError::IndexOutOfBounds { .. } => ErrorCode::E0108,
            NixError::CoercionError { .. } => ErrorCode::E0110,
            NixError::ArityMismatch { .. } => ErrorCode::E0409,

            // Module system errors
            NixError::UndefinedOption { .. } => ErrorCode::E0005,
            NixError::MissingDefinition { .. } => ErrorCode::E0004,
            NixError::ConflictingDefinitions { .. } => ErrorCode::E0003,
            NixError::InvalidOptionType { .. } => ErrorCode::E0001,
            NixError::ReadOnlyViolation { .. } => ErrorCode::E0006,
            NixError::CircularDependency { .. } => ErrorCode::E0200,
            NixError::ModuleNotFound { .. } => ErrorCode::E0103,
            NixError::InvalidModule { .. } => ErrorCode::E0105,
            NixError::ModuleClassMismatch { .. } => ErrorCode::E0007,
            NixError::InfiniteRecursion { .. } => ErrorCode::E0008,

            // External errors
            NixError::IoError { .. } => ErrorCode::E0100,
            NixError::FfiError { .. } => ErrorCode::E0500,
            NixError::Timeout { .. } => ErrorCode::E0106,

            // Internal errors
            NixError::InternalError { .. } => ErrorCode::E0900,
            NixError::NotImplemented { .. } => ErrorCode::E0901,

            // Multiple errors - use the first error's code
            NixError::Multiple { errors } => {
                errors.first().map(|e| e.code()).unwrap_or(ErrorCode::E0900)
            }
        }
    }

    /// Get the severity of this error.
    pub fn severity(&self) -> Severity {
        match self {
            // Most errors are actual errors
            _ => Severity::Error,
        }
    }

    /// Get the primary source span if available.
    pub fn span(&self) -> Option<&SourceSpan> {
        match self {
            NixError::UnexpectedToken { span, .. } => Some(span),
            NixError::UnexpectedEof { span, .. } => Some(span),
            NixError::InvalidEscape { span, .. } => Some(span),
            NixError::UnterminatedString { start_span, .. } => Some(start_span),
            NixError::InvalidNumber { span, .. } => Some(span),
            NixError::ParseError { span, .. } => Some(span),
            NixError::UndefinedVariable { span, .. } => Some(span),
            NixError::TypeMismatch { span, .. } => Some(span),
            NixError::AssertionFailed { span, .. } => Some(span),
            NixError::Throw { span, .. } => Some(span),
            NixError::Abort { span, .. } => Some(span),
            NixError::AttributeNotFound { span, .. } => Some(span),
            NixError::DivisionByZero { span, .. } => Some(span),
            NixError::IndexOutOfBounds { span, .. } => Some(span),
            NixError::CoercionError { span, .. } => Some(span),
            NixError::ArityMismatch { span, .. } => Some(span),
            NixError::UndefinedOption { span, .. } => span.as_ref(),
            NixError::MissingDefinition { declaration_span, .. } => declaration_span.as_ref(),
            NixError::ConflictingDefinitions { definitions, .. } => {
                definitions.first().map(|d| &d.span)
            }
            NixError::InvalidOptionType { span, .. } => span.as_ref(),
            NixError::ReadOnlyViolation { span, .. } => span.as_ref(),
            NixError::ModuleNotFound { import_span, .. } => import_span.as_ref(),
            NixError::InvalidModule { span, .. } => span.as_ref(),
            NixError::InternalError { span, .. } => span.as_ref(),
            NixError::NotImplemented { span, .. } => span.as_ref(),
            NixError::Multiple { errors } => errors.first().and_then(|e| e.span()),
            _ => None,
        }
    }

    /// Convert to a DiagnosticReport for rendering.
    pub fn to_diagnostic(&self) -> DiagnosticReport {
        match self {
            NixError::UnexpectedToken {
                expected,
                found,
                span,
            } => {
                let expected_str = if expected.len() == 1 {
                    expected[0].clone()
                } else {
                    format!("one of: {}", expected.join(", "))
                };
                DiagnosticReport::error(
                    self.code(),
                    format!("expected {}, found `{}`", expected_str, found),
                )
                .with_primary_label(span.clone(), format!("unexpected `{}`", found))
            }

            NixError::UnexpectedEof { expected, span } => {
                let expected_str = if expected.len() == 1 {
                    expected[0].clone()
                } else {
                    format!("one of: {}", expected.join(", "))
                };
                DiagnosticReport::error(self.code(), format!("unexpected end of file"))
                    .with_primary_label(span.clone(), format!("expected {}", expected_str))
            }

            NixError::InvalidEscape { sequence, span } => {
                DiagnosticReport::error(self.code(), format!("invalid escape sequence: {}", sequence))
                    .with_primary_label(span.clone(), "invalid escape")
            }

            NixError::UnterminatedString { start_span } => {
                DiagnosticReport::error(self.code(), "unterminated string literal")
                    .with_primary_label(start_span.clone(), "string starts here")
                    .with_help("add a closing `\"` to terminate the string")
            }

            NixError::InvalidNumber { literal, span } => {
                DiagnosticReport::error(self.code(), format!("invalid number: `{}`", literal))
                    .with_primary_label(span.clone(), "invalid number literal")
            }

            NixError::ParseError {
                message,
                span,
                hints,
            } => {
                let mut report =
                    DiagnosticReport::error(self.code(), message).with_primary_label(span.clone(), message);
                for hint in hints {
                    report = report.with_help(hint);
                }
                report
            }

            NixError::UndefinedVariable {
                name,
                span,
                similar,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("undefined variable `{}`", name),
                )
                .with_primary_label(span.clone(), "not found in this scope");

                if !similar.is_empty() {
                    let suggestions = similar
                        .iter()
                        .take(3)
                        .map(|s| format!("`{}`", s))
                        .collect::<Vec<_>>()
                        .join(" or ");
                    report = report.with_help(format!("did you mean {}?", suggestions));
                }

                report
            }

            NixError::TypeMismatch {
                expected,
                found,
                span,
                context,
            } => {
                let msg = if let Some(ctx) = context {
                    format!("expected {}, found {} ({})", expected, found, ctx)
                } else {
                    format!("expected {}, found {}", expected, found)
                };
                DiagnosticReport::error(self.code(), &msg)
                    .with_primary_label(span.clone(), format!("expected {}", expected))
            }

            NixError::AssertionFailed { message, span } => {
                let msg = message
                    .as_ref()
                    .map(|m| format!("assertion failed: {}", m))
                    .unwrap_or_else(|| "assertion failed".to_string());
                DiagnosticReport::error(self.code(), msg)
                    .with_primary_label(span.clone(), "assertion is false")
            }

            NixError::Throw { message, span } => {
                DiagnosticReport::error(self.code(), format!("evaluation error: {}", message))
                    .with_primary_label(span.clone(), "throw called here")
            }

            NixError::Abort { message, span } => {
                DiagnosticReport::error(self.code(), format!("evaluation aborted: {}", message))
                    .with_primary_label(span.clone(), "abort called here")
            }

            NixError::AttributeNotFound {
                name,
                attrset_type,
                span,
                available,
            } => {
                let msg = if let Some(typ) = attrset_type {
                    format!("attribute `{}` not found in {}", name, typ)
                } else {
                    format!("attribute `{}` not found", name)
                };
                let mut report = DiagnosticReport::error(self.code(), &msg)
                    .with_primary_label(span.clone(), "not found");

                if !available.is_empty() {
                    let suggestions = super::find_similar(name, available, 3);
                    if !suggestions.is_empty() {
                        report = report.with_help(format!(
                            "did you mean {}?",
                            suggestions
                                .iter()
                                .map(|s| format!("`{}`", s))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ));
                    }
                }

                report
            }

            NixError::DivisionByZero { span } => {
                DiagnosticReport::error(self.code(), "division by zero")
                    .with_primary_label(span.clone(), "division by zero here")
            }

            NixError::IndexOutOfBounds {
                index,
                length,
                span,
            } => DiagnosticReport::error(
                self.code(),
                format!(
                    "index {} out of bounds for list of length {}",
                    index, length
                ),
            )
            .with_primary_label(span.clone(), format!("index {} is out of bounds", index)),

            NixError::CoercionError { from, to, span } => {
                DiagnosticReport::error(self.code(), format!("cannot coerce {} to {}", from, to))
                    .with_primary_label(span.clone(), format!("expected {}", to))
            }

            NixError::ArityMismatch {
                expected,
                found,
                span,
            } => DiagnosticReport::error(
                self.code(),
                format!(
                    "function expects {} arguments, but {} were supplied",
                    expected, found
                ),
            )
            .with_primary_label(span.clone(), "arity mismatch"),

            NixError::UndefinedOption {
                path,
                span,
                available,
            } => {
                let mut report =
                    DiagnosticReport::error(self.code(), format!("undefined option `{}`", path));

                if let Some(s) = span {
                    report = report.with_primary_label(s.clone(), "undefined option");
                }

                if !available.is_empty() {
                    let last = path.split('.').last().unwrap_or(path);
                    let suggestions = super::find_similar(last, available, 3);
                    if !suggestions.is_empty() {
                        report = report.with_help(format!(
                            "did you mean {}?",
                            suggestions
                                .iter()
                                .map(|s| format!("`{}`", s))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ));
                    }
                }

                report
            }

            NixError::MissingDefinition {
                path,
                declaration_span,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("option `{}` is used but not defined", path),
                );

                if let Some(s) = declaration_span {
                    report = report.with_secondary_label(s.clone(), "option declared here");
                }

                report
            }

            NixError::ConflictingDefinitions {
                path,
                definitions,
                values,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("conflicting definitions for option `{}`", path),
                )
                .with_note(format!("{} conflicting values", values.len()));

                for (i, def) in definitions.iter().enumerate() {
                    let value_preview = values.get(i).map(|v| {
                        if v.len() > 50 {
                            format!("{}...", &v[..47])
                        } else {
                            v.clone()
                        }
                    });

                    let msg = if let Some(preview) = value_preview {
                        format!("{}: {}", def.message, preview)
                    } else {
                        def.message.clone()
                    };

                    report = report.with_label(Label {
                        span: def.span.clone(),
                        message: msg,
                        style: if i == 0 {
                            LabelStyle::Primary
                        } else {
                            LabelStyle::Secondary
                        },
                    });
                }

                report.with_help("use mkForce to override, or ensure values are compatible")
            }

            NixError::InvalidOptionType {
                path,
                expected,
                found,
                span,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!(
                        "option `{}` has type {}, but value is {}",
                        path, expected, found
                    ),
                );

                if let Some(s) = span {
                    report = report.with_primary_label(s.clone(), format!("expected {}", expected));
                }

                report
            }

            NixError::ReadOnlyViolation {
                path,
                span,
                declaration_span,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("cannot modify read-only option `{}`", path),
                );

                if let Some(s) = span {
                    report = report.with_primary_label(s.clone(), "attempted modification");
                }

                if let Some(s) = declaration_span {
                    report = report.with_secondary_label(s.clone(), "option is read-only");
                }

                report
            }

            NixError::CircularDependency { cycle } => {
                let cycle_str = cycle
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                DiagnosticReport::error(self.code(), "circular module dependency")
                    .with_note(format!("cycle: {}", cycle_str))
            }

            NixError::ModuleNotFound {
                path,
                search_paths,
                import_span,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("module not found: {}", path.display()),
                );

                if let Some(s) = import_span {
                    report = report.with_primary_label(s.clone(), "imported here");
                }

                if !search_paths.is_empty() {
                    report = report.with_note(format!(
                        "searched in: {}",
                        search_paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }

                report
            }

            NixError::InvalidModule {
                path,
                message,
                span,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("invalid module {}: {}", path.display(), message),
                );

                if let Some(s) = span {
                    report = report.with_primary_label(s.clone(), message);
                }

                report
            }

            NixError::ModuleClassMismatch {
                expected,
                found,
                path,
            } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("module class mismatch: expected {}, found {}", expected, found),
                );

                if let Some(p) = path {
                    report = report.with_note(format!("in module {}", p.display()));
                }

                report
            }

            NixError::InfiniteRecursion { path, cycle } => {
                let msg = if let Some(p) = path {
                    format!("infinite recursion detected at `{}`", p)
                } else {
                    "infinite recursion detected".to_string()
                };

                let mut report = DiagnosticReport::error(self.code(), msg);

                if !cycle.is_empty() {
                    report = report.with_note(format!("cycle: {}", cycle.join(" -> ")));
                }

                report
            }

            NixError::IoError { path, message } => {
                let msg = if let Some(p) = path {
                    format!("IO error for {}: {}", p.display(), message)
                } else {
                    format!("IO error: {}", message)
                };
                DiagnosticReport::error(self.code(), msg)
            }

            NixError::FfiError {
                message,
                external_code,
            } => {
                let mut report = DiagnosticReport::error(self.code(), message);
                if let Some(code) = external_code {
                    report = report.with_note(format!("external error code: {}", code));
                }
                report
            }

            NixError::Timeout { seconds, context } => {
                let msg = if let Some(ctx) = context {
                    format!("evaluation timed out after {} seconds ({})", seconds, ctx)
                } else {
                    format!("evaluation timed out after {} seconds", seconds)
                };
                DiagnosticReport::error(self.code(), msg)
            }

            NixError::InternalError { message, span } => {
                let mut report = DiagnosticReport::error(self.code(), format!("internal error: {}", message))
                    .with_note("this is a bug in the module system, please report it");

                if let Some(s) = span {
                    report = report.with_primary_label(s.clone(), "error occurred here");
                }

                report
            }

            NixError::NotImplemented { feature, span } => {
                let mut report = DiagnosticReport::error(
                    self.code(),
                    format!("not implemented: {}", feature),
                );

                if let Some(s) = span {
                    report = report.with_primary_label(s.clone(), "this feature is not yet implemented");
                }

                report
            }

            NixError::Multiple { errors } => {
                if errors.is_empty() {
                    return DiagnosticReport::error(ErrorCode::E0900, "no errors");
                }

                let first = errors[0].to_diagnostic();
                let mut report = first;

                for error in errors.iter().skip(1) {
                    report = report.with_child(error.to_diagnostic());
                }

                report
            }
        }
    }

    /// Create a parse error from the parser's ParseError type.
    pub fn from_parse_error(err: &crate::parse::ParseError) -> Self {
        NixError::ParseError {
            message: err.message.clone(),
            span: SourceSpan::from(&err.span),
            hints: err.hints.clone(),
        }
    }

    /// Create a multiple error from a list of errors.
    pub fn multiple(errors: Vec<NixError>) -> Self {
        if errors.len() == 1 {
            errors.into_iter().next().unwrap()
        } else {
            NixError::Multiple { errors }
        }
    }

    /// Check if this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        !matches!(
            self,
            NixError::InternalError { .. }
                | NixError::Abort { .. }
                | NixError::InfiniteRecursion { .. }
                | NixError::Timeout { .. }
        )
    }

    /// Convert to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Convert to pretty-printed JSON string.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

impl fmt::Display for NixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NixError::UnexpectedToken {
                expected, found, ..
            } => {
                write!(
                    f,
                    "unexpected token `{}`, expected {}",
                    found,
                    expected.join(" or ")
                )
            }
            NixError::UnexpectedEof { expected, .. } => {
                write!(f, "unexpected end of file, expected {}", expected.join(" or "))
            }
            NixError::InvalidEscape { sequence, .. } => {
                write!(f, "invalid escape sequence: {}", sequence)
            }
            NixError::UnterminatedString { .. } => {
                write!(f, "unterminated string literal")
            }
            NixError::InvalidNumber { literal, .. } => {
                write!(f, "invalid number: {}", literal)
            }
            NixError::ParseError { message, .. } => {
                write!(f, "{}", message)
            }
            NixError::UndefinedVariable { name, .. } => {
                write!(f, "undefined variable `{}`", name)
            }
            NixError::TypeMismatch {
                expected, found, ..
            } => {
                write!(f, "type mismatch: expected {}, found {}", expected, found)
            }
            NixError::AssertionFailed { message, .. } => {
                if let Some(msg) = message {
                    write!(f, "assertion failed: {}", msg)
                } else {
                    write!(f, "assertion failed")
                }
            }
            NixError::Throw { message, .. } => {
                write!(f, "evaluation error: {}", message)
            }
            NixError::Abort { message, .. } => {
                write!(f, "evaluation aborted: {}", message)
            }
            NixError::AttributeNotFound { name, .. } => {
                write!(f, "attribute `{}` not found", name)
            }
            NixError::DivisionByZero { .. } => {
                write!(f, "division by zero")
            }
            NixError::IndexOutOfBounds { index, length, .. } => {
                write!(
                    f,
                    "index {} out of bounds for list of length {}",
                    index, length
                )
            }
            NixError::CoercionError { from, to, .. } => {
                write!(f, "cannot coerce {} to {}", from, to)
            }
            NixError::ArityMismatch {
                expected, found, ..
            } => {
                write!(
                    f,
                    "function expects {} arguments, but {} were supplied",
                    expected, found
                )
            }
            NixError::UndefinedOption { path, .. } => {
                write!(f, "undefined option `{}`", path)
            }
            NixError::MissingDefinition { path, .. } => {
                write!(f, "option `{}` is used but not defined", path)
            }
            NixError::ConflictingDefinitions { path, .. } => {
                write!(f, "conflicting definitions for option `{}`", path)
            }
            NixError::InvalidOptionType {
                path,
                expected,
                found,
                ..
            } => {
                write!(
                    f,
                    "option `{}` has type {}, but value is {}",
                    path, expected, found
                )
            }
            NixError::ReadOnlyViolation { path, .. } => {
                write!(f, "cannot modify read-only option `{}`", path)
            }
            NixError::CircularDependency { cycle } => {
                write!(
                    f,
                    "circular module dependency: {}",
                    cycle
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" -> ")
                )
            }
            NixError::ModuleNotFound { path, .. } => {
                write!(f, "module not found: {}", path.display())
            }
            NixError::InvalidModule { path, message, .. } => {
                write!(f, "invalid module {}: {}", path.display(), message)
            }
            NixError::ModuleClassMismatch {
                expected, found, ..
            } => {
                write!(
                    f,
                    "module class mismatch: expected {}, found {}",
                    expected, found
                )
            }
            NixError::InfiniteRecursion { path, .. } => {
                if let Some(p) = path {
                    write!(f, "infinite recursion detected at `{}`", p)
                } else {
                    write!(f, "infinite recursion detected")
                }
            }
            NixError::IoError { path, message } => {
                if let Some(p) = path {
                    write!(f, "IO error for {}: {}", p.display(), message)
                } else {
                    write!(f, "IO error: {}", message)
                }
            }
            NixError::FfiError { message, .. } => {
                write!(f, "FFI error: {}", message)
            }
            NixError::Timeout { seconds, .. } => {
                write!(f, "evaluation timed out after {} seconds", seconds)
            }
            NixError::InternalError { message, .. } => {
                write!(f, "internal error: {}", message)
            }
            NixError::NotImplemented { feature, .. } => {
                write!(f, "not implemented: {}", feature)
            }
            NixError::Multiple { errors } => {
                write!(
                    f,
                    "{} errors",
                    errors.len()
                )
            }
        }
    }
}

impl std::error::Error for NixError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nix_error_code() {
        let err = NixError::UndefinedVariable {
            name: "foo".to_string(),
            span: SourceSpan::virtual_span("test"),
            similar: vec![],
        };
        assert_eq!(err.code(), ErrorCode::E0400);
    }

    #[test]
    fn test_nix_error_span() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let err = NixError::UndefinedVariable {
            name: "foo".to_string(),
            span: span.clone(),
            similar: vec![],
        };
        assert_eq!(err.span(), Some(&span));
    }

    #[test]
    fn test_nix_error_display() {
        let err = NixError::UndefinedVariable {
            name: "foo".to_string(),
            span: SourceSpan::virtual_span("test"),
            similar: vec![],
        };
        assert_eq!(format!("{}", err), "undefined variable `foo`");
    }

    #[test]
    fn test_nix_error_to_diagnostic() {
        let err = NixError::TypeMismatch {
            expected: "bool".to_string(),
            found: "string".to_string(),
            span: SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3),
            context: None,
        };

        let diag = err.to_diagnostic();
        assert_eq!(diag.code, ErrorCode::E0001);
        assert!(diag.message.contains("bool"));
        assert!(diag.message.contains("string"));
    }

    #[test]
    fn test_nix_error_serialization() {
        let err = NixError::UndefinedVariable {
            name: "foo".to_string(),
            span: SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3),
            similar: vec!["foobar".to_string()],
        };

        let json = err.to_json().unwrap();
        assert!(json.contains("\"kind\":\"UndefinedVariable\""));
        assert!(json.contains("\"name\":\"foo\""));

        let deserialized: NixError = serde_json::from_str(&json).unwrap();
        if let NixError::UndefinedVariable { name, .. } = deserialized {
            assert_eq!(name, "foo");
        } else {
            panic!("Wrong error type after deserialization");
        }
    }

    #[test]
    fn test_nix_error_multiple() {
        let errors = vec![
            NixError::UndefinedVariable {
                name: "foo".to_string(),
                span: SourceSpan::virtual_span("test"),
                similar: vec![],
            },
            NixError::UndefinedVariable {
                name: "bar".to_string(),
                span: SourceSpan::virtual_span("test"),
                similar: vec![],
            },
        ];

        let multi = NixError::multiple(errors);
        if let NixError::Multiple { errors } = &multi {
            assert_eq!(errors.len(), 2);
        } else {
            panic!("Expected Multiple variant");
        }
    }

    #[test]
    fn test_nix_error_single_multiple() {
        let errors = vec![NixError::UndefinedVariable {
            name: "foo".to_string(),
            span: SourceSpan::virtual_span("test"),
            similar: vec![],
        }];

        let single = NixError::multiple(errors);
        // Single error should be unwrapped
        assert!(matches!(single, NixError::UndefinedVariable { .. }));
    }

    #[test]
    fn test_conflicting_definitions_diagnostic() {
        let span1 = SourceSpan::new(PathBuf::from("a.nix"), 10, 20, 5, 3);
        let span2 = SourceSpan::new(PathBuf::from("b.nix"), 30, 40, 10, 5);

        let err = NixError::ConflictingDefinitions {
            path: "services.nginx.enable".to_string(),
            definitions: vec![
                RelatedLocation::new(span1, "defined here"),
                RelatedLocation::new(span2, "also defined here"),
            ],
            values: vec!["true".to_string(), "false".to_string()],
        };

        let diag = err.to_diagnostic();
        assert_eq!(diag.code, ErrorCode::E0003);
        assert_eq!(diag.labels.len(), 2);
    }

    #[test]
    fn test_is_recoverable() {
        let recoverable = NixError::UndefinedVariable {
            name: "foo".to_string(),
            span: SourceSpan::virtual_span("test"),
            similar: vec![],
        };
        assert!(recoverable.is_recoverable());

        let non_recoverable = NixError::Abort {
            message: "fatal error".to_string(),
            span: SourceSpan::virtual_span("test"),
        };
        assert!(!non_recoverable.is_recoverable());
    }
}
