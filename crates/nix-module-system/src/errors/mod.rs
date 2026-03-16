//! Error types and error rendering with ariadne.
//!
//! This module provides a unified error type hierarchy for all error sources:
//! - Nix syntax errors (from parser)
//! - Nix semantic/evaluation errors (undefined variables, type errors, assertion failures)
//! - Module system errors (type mismatches, undefined options, merge conflicts, circular imports)
//!
//! All errors are:
//! - Machine-readable (Serialize/Deserialize for JSON output)
//! - Source-mapped (file path, line, column, byte span)
//! - Severity-tagged (Error, Warning, Info, Hint)
//! - Multi-span capable (related locations for context)
//! - Ready for ariadne integration

// Allow missing docs on enum variant fields since the #[error] attributes document them
#![allow(missing_docs)]

mod codes;
mod diagnostic;
mod nix_error;
mod render;
mod span;

pub use codes::*;
pub use diagnostic::*;
pub use nix_error::*;
pub use render::*;
pub use span::*;

use crate::types::{OptionPath, Value};
use std::path::PathBuf;
use thiserror::Error;

/// Result type for type operations
pub type TypeResult<T> = Result<T, TypeError>;

/// Result type for Nix operations
pub type NixResult<T> = Result<T, NixError>;

/// Errors that can occur during type checking and merging
#[derive(Debug, Error, Clone)]
pub enum TypeError {
    /// Type mismatch between expected and actual value
    #[error("expected {expected}, found {found}")]
    Mismatch {
        expected: String,
        found: String,
        value: Option<Value>,
    },

    /// Enum value not in allowed set
    #[error("expected one of {}, found \"{found}\"", expected.join(", "))]
    EnumMismatch { expected: Vec<String>, found: String },

    /// Conflicting definitions that cannot be merged
    #[error("conflicting definitions for option `{path}`")]
    ConflictingDefinitions { path: OptionPath, values: Vec<Value> },

    /// Option has no definition and no default
    #[error("option `{path}` is used but not defined")]
    NoDefinition { path: OptionPath },

    /// Undefined option in a submodule
    #[error("undefined option `{path}`")]
    UndefinedOption {
        path: OptionPath,
        available: Vec<String>,
    },

    /// Feature not yet implemented
    #[error("unsupported feature: {feature}")]
    UnsupportedFeature { feature: String },

    /// Read-only option was modified
    #[error("option `{path}` is read-only")]
    ReadOnlyViolation { path: OptionPath },

    /// Module class mismatch
    #[error("module class mismatch: expected {expected}, found {found}")]
    ClassMismatch { expected: String, found: String },

    /// Infinite recursion detected
    #[error("infinite recursion in module evaluation")]
    InfiniteRecursion { path: OptionPath, cycle: Vec<String> },
}

impl TypeError {
    /// Add path context to an error
    pub fn at_path(self, path: OptionPath) -> Self {
        match self {
            TypeError::Mismatch {
                expected,
                found,
                value,
            } => TypeError::Mismatch {
                expected: format!("{} at `{}`", expected, path),
                found,
                value,
            },
            other => other,
        }
    }

    /// Get the error code for this error
    pub fn code(&self) -> ErrorCode {
        match self {
            TypeError::Mismatch { .. } => ErrorCode::E0001,
            TypeError::EnumMismatch { .. } => ErrorCode::E0002,
            TypeError::ConflictingDefinitions { .. } => ErrorCode::E0003,
            TypeError::NoDefinition { .. } => ErrorCode::E0004,
            TypeError::UndefinedOption { .. } => ErrorCode::E0005,
            TypeError::ReadOnlyViolation { .. } => ErrorCode::E0006,
            TypeError::ClassMismatch { .. } => ErrorCode::E0007,
            TypeError::InfiniteRecursion { .. } => ErrorCode::E0008,
            TypeError::UnsupportedFeature { .. } => ErrorCode::E0099,
        }
    }
}

/// Errors that can occur during module evaluation
#[derive(Debug, Error, Clone)]
pub enum EvalError {
    /// Type error during evaluation
    #[error(transparent)]
    Type(#[from] TypeError),

    /// IO error with context
    #[error("IO error for {path}: {message}")]
    Io { path: PathBuf, message: String },

    /// Parse error in Nix file
    #[error("parse error in {file}: {message}")]
    Parse {
        file: PathBuf,
        message: String,
        span: Option<SourceSpan>,
    },

    /// Import cycle detected
    #[error("import cycle detected: {}", cycle.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" -> "))]
    ImportCycle { cycle: Vec<PathBuf> },

    /// Module not found
    #[error("module not found: {}", path.display())]
    ModuleNotFound {
        path: PathBuf,
        search_paths: Vec<PathBuf>,
    },

    /// Import not found
    #[error("import `{}` not found (from {})", import_path.display(), from_module.display())]
    ImportNotFound {
        import_path: PathBuf,
        from_module: PathBuf,
    },

    /// Invalid module structure
    #[error("invalid module structure in {file}: {message}")]
    InvalidModule { file: PathBuf, message: String },

    /// Evaluation timeout
    #[error("evaluation timed out after {seconds} seconds")]
    Timeout { seconds: u64 },
}

impl EvalError {
    /// Get the error code for this error
    pub fn code(&self) -> ErrorCode {
        match self {
            EvalError::Type(te) => te.code(),
            EvalError::Io { .. } => ErrorCode::E0100,
            EvalError::Parse { .. } => ErrorCode::E0101,
            EvalError::ImportCycle { .. } => ErrorCode::E0102,
            EvalError::ModuleNotFound { .. } => ErrorCode::E0103,
            EvalError::ImportNotFound { .. } => ErrorCode::E0104,
            EvalError::InvalidModule { .. } => ErrorCode::E0105,
            EvalError::Timeout { .. } => ErrorCode::E0106,
        }
    }
}

/// Find similar strings using Levenshtein distance
pub fn find_similar(target: &str, candidates: &[String], max_results: usize) -> Vec<String> {
    let mut scored: Vec<_> = candidates
        .iter()
        .map(|c| (c.clone(), levenshtein(target, c)))
        .filter(|(_, d)| *d <= 3) // Only consider close matches
        .collect();

    scored.sort_by_key(|(_, d)| *d);
    scored.truncate(max_results);

    scored.into_iter().map(|(s, _)| s).collect()
}

/// Simple Levenshtein distance implementation
fn levenshtein(a: &str, b: &str) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;

        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1)
                .min(curr[j] + 1)
                .min(prev[j] + cost);
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("hello", "hello"), 0);
        assert_eq!(levenshtein("hello", "helo"), 1);
        assert_eq!(levenshtein("hello", "world"), 4);
    }

    #[test]
    fn test_find_similar() {
        let candidates = vec![
            "enable".to_string(),
            "enabled".to_string(),
            "disable".to_string(),
            "foo".to_string(),
        ];

        let similar = find_similar("enabl", &candidates, 2);
        assert!(similar.contains(&"enable".to_string()));
    }

    #[test]
    fn test_type_error_code() {
        let err = TypeError::NoDefinition {
            path: OptionPath::new(vec!["test".into()]),
        };
        assert_eq!(err.code(), ErrorCode::E0004);
    }

    #[test]
    fn test_eval_error_code() {
        let err = EvalError::Timeout { seconds: 30 };
        assert_eq!(err.code(), ErrorCode::E0106);
    }
}
