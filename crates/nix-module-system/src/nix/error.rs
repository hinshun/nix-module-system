//! Error types for Nix evaluation.
//!
//! This module provides error types that bridge between nix-bindings-rust errors
//! and our unified error types.

use crate::errors::{Diagnostic, EvalError, Severity, SourceLocation};
use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during Nix evaluation.
#[derive(Debug, Error)]
pub enum NixError {
    /// Initialization error - failed to initialize Nix library.
    #[error("failed to initialize Nix library: {message}")]
    InitError {
        /// The error message.
        message: String,
    },

    /// Thread registration error.
    #[error("failed to register thread with Nix GC: {message}")]
    ThreadRegistrationError {
        /// The error message.
        message: String,
    },

    /// Store error - failed to open Nix store.
    #[error("failed to open Nix store: {message}")]
    StoreError {
        /// The error message.
        message: String,
    },

    /// Parse error - failed to parse Nix expression.
    #[error("parse error in {file}: {message}")]
    ParseError {
        /// The file that failed to parse.
        file: PathBuf,
        /// The error message.
        message: String,
        /// Line number (1-indexed), if available.
        line: Option<usize>,
        /// Column number (1-indexed), if available.
        column: Option<usize>,
    },

    /// Evaluation error - error during Nix evaluation.
    #[error("evaluation error: {message}")]
    EvaluationError {
        /// The error message.
        message: String,
        /// Stack trace frames.
        trace: Vec<TraceFrame>,
    },

    /// Type error - unexpected type from Nix evaluation.
    #[error("type error: expected {expected}, got {actual}")]
    TypeError {
        /// The expected type.
        expected: String,
        /// The actual type found.
        actual: String,
    },

    /// Attribute not found.
    #[error("attribute `{name}` not found in {path}")]
    AttributeNotFound {
        /// The attribute name that was not found.
        name: String,
        /// The path to the attribute set.
        path: String,
    },

    /// File not found.
    #[error("file not found: {}", path.display())]
    FileNotFound {
        /// The path that was not found.
        path: PathBuf,
    },

    /// IO error.
    #[error("IO error: {message}")]
    IoError {
        /// The error message.
        message: String,
    },

    /// Value conversion error.
    #[error("failed to convert value: {message}")]
    ConversionError {
        /// The conversion error message.
        message: String,
    },

    /// Generic error from nix-bindings-rust.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// A frame in the Nix evaluation trace.
#[derive(Debug, Clone)]
pub struct TraceFrame {
    /// The file where the error occurred.
    pub file: Option<PathBuf>,
    /// Line number (1-indexed).
    pub line: Option<usize>,
    /// Column number (1-indexed).
    pub column: Option<usize>,
    /// Description of the frame.
    pub description: String,
}

impl TraceFrame {
    /// Create a new trace frame.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            file: None,
            line: None,
            column: None,
            description: description.into(),
        }
    }

    /// Add file location to the frame.
    pub fn with_location(mut self, file: PathBuf, line: usize, column: usize) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self.column = Some(column);
        self
    }
}

/// Result type for Nix operations.
pub type NixResult<T> = Result<T, NixError>;

impl NixError {
    /// Create a parse error from components.
    pub fn parse(file: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::ParseError {
            file: file.into(),
            message: message.into(),
            line: None,
            column: None,
        }
    }

    /// Create an evaluation error.
    pub fn evaluation(message: impl Into<String>) -> Self {
        Self::EvaluationError {
            message: message.into(),
            trace: Vec::new(),
        }
    }

    /// Create an evaluation error with trace.
    pub fn evaluation_with_trace(message: impl Into<String>, trace: Vec<TraceFrame>) -> Self {
        Self::EvaluationError {
            message: message.into(),
            trace,
        }
    }

    /// Create a type error.
    pub fn type_error(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::TypeError {
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    /// Create a conversion error.
    pub fn conversion(message: impl Into<String>) -> Self {
        Self::ConversionError {
            message: message.into(),
        }
    }

    /// Check if this is a recoverable error.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            NixError::AttributeNotFound { .. } | NixError::TypeError { .. }
        )
    }

    /// Get the primary source location if available.
    pub fn source_location(&self) -> Option<SourceLocation> {
        match self {
            NixError::ParseError {
                file,
                line,
                column,
                ..
            } => {
                let line = (*line)?;
                let column = (*column)?;
                Some(SourceLocation {
                    file: file.clone(),
                    start: 0,
                    end: 0,
                    line,
                    column,
                })
            }
            NixError::EvaluationError { trace, .. } => {
                trace.first().and_then(|frame| {
                    let file = frame.file.clone()?;
                    Some(SourceLocation {
                        file,
                        start: 0,
                        end: 0,
                        line: frame.line.unwrap_or(1),
                        column: frame.column.unwrap_or(1),
                    })
                })
            }
            _ => None,
        }
    }
}

impl From<NixError> for EvalError {
    fn from(err: NixError) -> Self {
        match err {
            NixError::ParseError { file, message, .. } => EvalError::Parse {
                file,
                message,
                span: None,
            },
            NixError::FileNotFound { path } => EvalError::ModuleNotFound {
                path,
                search_paths: Vec::new(),
            },
            NixError::IoError { message } => EvalError::Io {
                path: PathBuf::new(),
                message,
            },
            other => EvalError::Io {
                path: PathBuf::new(),
                message: other.to_string(),
            },
        }
    }
}

impl From<NixError> for Diagnostic {
    fn from(err: NixError) -> Self {
        let mut diag = Diagnostic {
            severity: Severity::Error,
            code: Some(error_code(&err)),
            message: err.to_string(),
            primary: err.source_location(),
            secondary: Vec::new(),
            notes: Vec::new(),
        };

        // Add trace frames as secondary annotations
        if let NixError::EvaluationError { trace, .. } = &err {
            for (i, frame) in trace.iter().skip(1).enumerate() {
                if let Some(file) = &frame.file {
                    diag.secondary.push((
                        SourceLocation {
                            file: file.clone(),
                            start: 0,
                            end: 0,
                            line: frame.line.unwrap_or(1),
                            column: frame.column.unwrap_or(1),
                        },
                        format!("trace {}: {}", i + 1, frame.description),
                    ));
                }
            }
        }

        diag
    }
}

/// Get error code for a NixError.
fn error_code(err: &NixError) -> String {
    match err {
        NixError::InitError { .. } => "N0001".to_string(),
        NixError::ThreadRegistrationError { .. } => "N0002".to_string(),
        NixError::StoreError { .. } => "N0003".to_string(),
        NixError::ParseError { .. } => "N0010".to_string(),
        NixError::EvaluationError { .. } => "N0020".to_string(),
        NixError::TypeError { .. } => "N0021".to_string(),
        NixError::AttributeNotFound { .. } => "N0022".to_string(),
        NixError::FileNotFound { .. } => "N0030".to_string(),
        NixError::IoError { .. } => "N0031".to_string(),
        NixError::ConversionError { .. } => "N0040".to_string(),
        NixError::Other(_) => "N0099".to_string(),
    }
}

/// Convert this FFI NixError to the unified errors::NixError type.
///
/// This allows all errors to be rendered through the same diagnostic system.
impl NixError {
    /// Convert to the unified NixError type from errors module.
    pub fn to_unified_error(&self) -> crate::errors::NixError {
        use crate::errors::{NixError as UnifiedError, SourceSpan};

        match self {
            NixError::InitError { message } => UnifiedError::FfiError {
                message: message.clone(),
                external_code: Some("N0001".to_string()),
            },
            NixError::ThreadRegistrationError { message } => UnifiedError::FfiError {
                message: message.clone(),
                external_code: Some("N0002".to_string()),
            },
            NixError::StoreError { message } => UnifiedError::FfiError {
                message: message.clone(),
                external_code: Some("N0003".to_string()),
            },
            NixError::ParseError {
                file,
                message,
                line,
                column,
            } => {
                let span = SourceSpan::new(
                    file.clone(),
                    0,
                    0,
                    line.unwrap_or(1),
                    column.unwrap_or(1),
                );
                UnifiedError::ParseError {
                    message: message.clone(),
                    span,
                    hints: Vec::new(),
                }
            }
            NixError::EvaluationError { message, trace } => {
                let span = trace.first().and_then(|frame| {
                    frame.file.as_ref().map(|f| {
                        SourceSpan::new(
                            f.clone(),
                            0,
                            0,
                            frame.line.unwrap_or(1),
                            frame.column.unwrap_or(1),
                        )
                    })
                });

                if let Some(span) = span {
                    UnifiedError::Throw {
                        message: message.clone(),
                        span,
                    }
                } else {
                    UnifiedError::FfiError {
                        message: message.clone(),
                        external_code: Some("N0020".to_string()),
                    }
                }
            }
            NixError::TypeError { expected, actual } => UnifiedError::FfiError {
                message: format!("type error: expected {}, got {}", expected, actual),
                external_code: Some("N0021".to_string()),
            },
            NixError::AttributeNotFound { name, path } => UnifiedError::FfiError {
                message: format!("attribute `{}` not found in {}", name, path),
                external_code: Some("N0022".to_string()),
            },
            NixError::FileNotFound { path } => UnifiedError::ModuleNotFound {
                path: path.clone(),
                search_paths: Vec::new(),
                import_span: None,
            },
            NixError::IoError { message } => UnifiedError::IoError {
                path: None,
                message: message.clone(),
            },
            NixError::ConversionError { message } => UnifiedError::FfiError {
                message: message.clone(),
                external_code: Some("N0040".to_string()),
            },
            NixError::Other(e) => UnifiedError::FfiError {
                message: e.to_string(),
                external_code: Some("N0099".to_string()),
            },
        }
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = NixError::parse("/test.nix", "unexpected token");
        assert!(err.to_string().contains("parse error"));
        assert!(err.to_string().contains("/test.nix"));
    }

    #[test]
    fn test_error_with_trace() {
        let trace = vec![
            TraceFrame::new("while evaluating 'foo'")
                .with_location(PathBuf::from("/a.nix"), 10, 5),
            TraceFrame::new("called from 'bar'").with_location(PathBuf::from("/b.nix"), 20, 3),
        ];
        let err = NixError::evaluation_with_trace("undefined variable 'x'", trace);

        assert!(err.source_location().is_some());
        let loc = err.source_location().unwrap();
        assert_eq!(loc.line, 10);
    }

    #[test]
    fn test_error_to_diagnostic() {
        let err = NixError::type_error("string", "int");
        let diag: Diagnostic = err.into();
        assert_eq!(diag.severity, Severity::Error);
        assert!(diag.code.is_some());
    }
}
