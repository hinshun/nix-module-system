//! Diagnostic report types for rich error messages.
//!
//! This module provides the `DiagnosticReport` type which combines all
//! error information into a single, renderable structure.

use super::{ErrorCode, Label, RelatedLocation, SourceSpan};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Error - evaluation fails
    Error,
    /// Warning - evaluation continues, but something may be wrong
    Warning,
    /// Info - informational message
    Info,
    /// Hint - suggestion for improvement
    Hint,
}

impl Severity {
    /// Check if this severity is an error.
    pub fn is_error(&self) -> bool {
        matches!(self, Severity::Error)
    }

    /// Check if this severity is a warning or higher.
    pub fn is_warning_or_higher(&self) -> bool {
        matches!(self, Severity::Error | Severity::Warning)
    }

    /// Get the severity as a string for display.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
            Severity::Hint => "hint",
        }
    }
}

impl Default for Severity {
    fn default() -> Self {
        Severity::Error
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A suggestion for fixing an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Description of the suggestion
    pub message: String,

    /// The span to replace (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,

    /// The replacement text (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,

    /// Whether this is a machine-applicable fix
    pub applicability: Applicability,
}

impl Suggestion {
    /// Create a simple textual suggestion.
    pub fn help(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            replacement: None,
            applicability: Applicability::Unspecified,
        }
    }

    /// Create a machine-applicable replacement suggestion.
    pub fn replacement(
        message: impl Into<String>,
        span: SourceSpan,
        replacement: impl Into<String>,
        applicability: Applicability,
    ) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
            replacement: Some(replacement.into()),
            applicability,
        }
    }

    /// Check if this suggestion can be automatically applied.
    pub fn is_applicable(&self) -> bool {
        matches!(
            self.applicability,
            Applicability::MachineApplicable | Applicability::MaybeIncorrect
        )
    }
}

/// How applicable a suggestion is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    /// The suggestion is definitely correct and can be applied automatically.
    MachineApplicable,
    /// The suggestion might be correct but should be reviewed.
    MaybeIncorrect,
    /// The suggestion has placeholders that need to be filled in.
    HasPlaceholders,
    /// The applicability is not specified.
    Unspecified,
}

impl Default for Applicability {
    fn default() -> Self {
        Applicability::Unspecified
    }
}

/// A full diagnostic report with all context.
///
/// This is the main type for error reporting, combining:
/// - Error code and severity
/// - Primary message
/// - Labeled source spans
/// - Related locations
/// - Notes and suggestions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticReport {
    /// Error severity
    pub severity: Severity,

    /// Error code for programmatic handling
    pub code: ErrorCode,

    /// Primary error message (short, one line)
    pub message: String,

    /// Detailed explanation (optional, can be multi-line)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// Primary source location
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_span: Option<SourceSpan>,

    /// Labeled spans (primary and secondary)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub labels: Vec<Label>,

    /// Related locations (e.g., "defined here", "conflicting definition")
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related: Vec<RelatedLocation>,

    /// Help notes
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notes: Vec<String>,

    /// Suggestions for fixing the error
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub suggestions: Vec<Suggestion>,

    /// Child diagnostics (for grouped errors)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<DiagnosticReport>,
}

impl DiagnosticReport {
    /// Create a new error diagnostic.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            detail: None,
            primary_span: None,
            labels: Vec::new(),
            related: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Create a new warning diagnostic.
    pub fn warning(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            detail: None,
            primary_span: None,
            labels: Vec::new(),
            related: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Create a new info diagnostic.
    pub fn info(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            code,
            message: message.into(),
            detail: None,
            primary_span: None,
            labels: Vec::new(),
            related: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Create a new hint diagnostic.
    pub fn hint(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Hint,
            code,
            message: message.into(),
            detail: None,
            primary_span: None,
            labels: Vec::new(),
            related: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Set the detailed explanation.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Set the primary source span.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.primary_span = Some(span);
        self
    }

    /// Add a primary label at the given span.
    pub fn with_primary_label(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.labels.push(Label::primary(span, message));
        self
    }

    /// Add a secondary label at the given span.
    pub fn with_secondary_label(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.labels.push(Label::secondary(span, message));
        self
    }

    /// Add a label.
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// Add a related location.
    pub fn with_related(mut self, related: RelatedLocation) -> Self {
        self.related.push(related);
        self
    }

    /// Add a help note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Add a suggestion.
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Add a simple help suggestion.
    pub fn with_help(mut self, message: impl Into<String>) -> Self {
        self.suggestions.push(Suggestion::help(message));
        self
    }

    /// Add a child diagnostic.
    pub fn with_child(mut self, child: DiagnosticReport) -> Self {
        self.children.push(child);
        self
    }

    /// Check if this diagnostic has any source spans.
    pub fn has_spans(&self) -> bool {
        self.primary_span.is_some() || !self.labels.is_empty()
    }

    /// Get the primary span, or the first label span if no primary is set.
    pub fn primary_or_first_span(&self) -> Option<&SourceSpan> {
        self.primary_span
            .as_ref()
            .or_else(|| self.labels.first().map(|l| &l.span))
    }

    /// Get all file paths referenced by this diagnostic.
    pub fn referenced_files(&self) -> Vec<&std::path::Path> {
        let mut files = Vec::new();

        if let Some(span) = &self.primary_span {
            files.push(span.file.as_path());
        }

        for label in &self.labels {
            if !files.contains(&label.span.file.as_path()) {
                files.push(label.span.file.as_path());
            }
        }

        for related in &self.related {
            if !files.contains(&related.span.file.as_path()) {
                files.push(related.span.file.as_path());
            }
        }

        files
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

impl fmt::Display for DiagnosticReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}[{}]: {}", self.severity, self.code, self.message)?;

        if let Some(span) = &self.primary_span {
            write!(f, "\n  --> {}", span)?;
        }

        for note in &self.notes {
            write!(f, "\n  = help: {}", note)?;
        }

        for suggestion in &self.suggestions {
            write!(f, "\n  = suggestion: {}", suggestion.message)?;
        }

        Ok(())
    }
}

/// Legacy Diagnostic type for backward compatibility.
///
/// This type maps to the old Diagnostic interface while using the new types internally.
#[derive(Debug)]
pub struct Diagnostic {
    /// Error severity
    pub severity: Severity,
    /// Main error code
    pub code: Option<String>,
    /// Primary message
    pub message: String,
    /// Primary source location
    pub primary: Option<SourceLocation>,
    /// Secondary annotations
    pub secondary: Vec<(SourceLocation, String)>,
    /// Help notes
    pub notes: Vec<String>,
}

/// Legacy source location for backward compatibility.
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// File path
    pub file: std::path::PathBuf,
    /// Byte range start
    pub start: usize,
    /// Byte range end
    pub end: usize,
    /// Human-readable line
    pub line: usize,
    /// Human-readable column
    pub column: usize,
}

impl From<crate::types::Span> for SourceLocation {
    fn from(span: crate::types::Span) -> Self {
        Self {
            file: span.file,
            start: span.start,
            end: span.end,
            line: span.line,
            column: span.column,
        }
    }
}

impl From<SourceSpan> for SourceLocation {
    fn from(span: SourceSpan) -> Self {
        Self {
            file: span.file,
            start: span.start,
            end: span.end,
            line: span.line,
            column: span.column,
        }
    }
}

impl Diagnostic {
    /// Create a new error diagnostic.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            primary: None,
            secondary: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Create a new warning diagnostic.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: None,
            message: message.into(),
            primary: None,
            secondary: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Set the error code.
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Set the primary location.
    pub fn with_primary(mut self, loc: SourceLocation, label: impl Into<String>) -> Self {
        self.primary = Some(loc);
        self.message = label.into();
        self
    }

    /// Add a secondary annotation.
    pub fn with_secondary(mut self, loc: SourceLocation, label: impl Into<String>) -> Self {
        self.secondary.push((loc, label.into()));
        self
    }

    /// Add a help note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Convert to a DiagnosticReport.
    pub fn to_report(&self) -> DiagnosticReport {
        let code = self
            .code
            .as_ref()
            .and_then(|c| ErrorCode::try_from(c.clone()).ok())
            .unwrap_or(ErrorCode::E0900);

        let mut report = DiagnosticReport::error(code, &self.message);
        report.severity = self.severity;

        if let Some(primary) = &self.primary {
            let span = SourceSpan::new(
                primary.file.clone(),
                primary.start,
                primary.end,
                primary.line,
                primary.column,
            );
            report = report.with_primary_label(span, &self.message);
        }

        for (loc, label) in &self.secondary {
            let span = SourceSpan::new(
                loc.file.clone(),
                loc.start,
                loc.end,
                loc.line,
                loc.column,
            );
            report = report.with_secondary_label(span, label);
        }

        for note in &self.notes {
            report = report.with_note(note);
        }

        report
    }
}

impl From<super::TypeError> for Diagnostic {
    fn from(err: super::TypeError) -> Self {
        use super::{find_similar, TypeError};

        match err {
            TypeError::Mismatch {
                expected,
                found,
                value: _,
            } => Diagnostic::error(format!("expected {}, found {}", expected, found))
                .with_code("E0001"),

            TypeError::EnumMismatch { expected, found } => {
                Diagnostic::error(format!("invalid enum value: \"{}\"", found))
                    .with_code("E0002")
                    .with_note(format!("valid values are: {}", expected.join(", ")))
            }

            TypeError::ConflictingDefinitions { path, values } => {
                Diagnostic::error(format!("conflicting definitions for `{}`", path))
                    .with_code("E0003")
                    .with_note(format!("{} conflicting values found", values.len()))
            }

            TypeError::NoDefinition { path } => {
                Diagnostic::error(format!("option `{}` has no definition", path)).with_code("E0004")
            }

            TypeError::UndefinedOption { path, available } => {
                let mut diag = Diagnostic::error(format!("undefined option `{}`", path))
                    .with_code("E0005");

                // Suggest similar options
                if !available.is_empty() {
                    let last = path.components().last().map(|s| s.as_str()).unwrap_or("");
                    let suggestions = find_similar(last, &available, 3);
                    if !suggestions.is_empty() {
                        diag = diag.with_note(format!(
                            "did you mean {}?",
                            suggestions
                                .iter()
                                .map(|s| format!("`{}`", s))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ));
                    }
                }

                diag
            }

            TypeError::ReadOnlyViolation { path } => {
                Diagnostic::error(format!("cannot modify read-only option `{}`", path))
                    .with_code("E0006")
            }

            TypeError::UnsupportedFeature { feature } => {
                Diagnostic::error(format!("unsupported feature: {}", feature)).with_code("E0099")
            }

            TypeError::ClassMismatch { expected, found } => {
                Diagnostic::error(format!(
                    "module class mismatch: expected {}, found {}",
                    expected, found
                ))
                .with_code("E0007")
            }

            TypeError::InfiniteRecursion { path, cycle } => {
                Diagnostic::error(format!("infinite recursion at `{}`", path))
                    .with_code("E0008")
                    .with_note(format!("cycle: {}", cycle.join(" -> ")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", Severity::Error), "error");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Info), "info");
        assert_eq!(format!("{}", Severity::Hint), "hint");
    }

    #[test]
    fn test_severity_checks() {
        assert!(Severity::Error.is_error());
        assert!(!Severity::Warning.is_error());
        assert!(Severity::Error.is_warning_or_higher());
        assert!(Severity::Warning.is_warning_or_higher());
        assert!(!Severity::Info.is_warning_or_higher());
    }

    #[test]
    fn test_diagnostic_report_creation() {
        let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch");
        assert_eq!(report.severity, Severity::Error);
        assert_eq!(report.code, ErrorCode::E0001);
        assert_eq!(report.message, "type mismatch");
    }

    #[test]
    fn test_diagnostic_report_builder() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch")
            .with_span(span.clone())
            .with_primary_label(span.clone(), "expected bool")
            .with_note("booleans must be true or false")
            .with_help("try using `true` or `false`");

        assert!(report.primary_span.is_some());
        assert_eq!(report.labels.len(), 1);
        assert_eq!(report.notes.len(), 1);
        assert_eq!(report.suggestions.len(), 1);
    }

    #[test]
    fn test_diagnostic_report_display() {
        let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch")
            .with_note("expected bool");

        let output = format!("{}", report);
        assert!(output.contains("error[E0001]"));
        assert!(output.contains("type mismatch"));
        assert!(output.contains("expected bool"));
    }

    #[test]
    fn test_diagnostic_report_serialization() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch")
            .with_span(span)
            .with_note("expected bool");

        let json = report.to_json().unwrap();
        assert!(json.contains("\"severity\":\"error\""));
        assert!(json.contains("\"code\":\"E0001\""));
        assert!(json.contains("\"message\":\"type mismatch\""));
    }

    #[test]
    fn test_suggestion_creation() {
        let suggestion = Suggestion::help("try using mkForce");
        assert!(!suggestion.is_applicable());

        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let replacement = Suggestion::replacement(
            "use mkForce",
            span,
            "mkForce true",
            Applicability::MachineApplicable,
        );
        assert!(replacement.is_applicable());
    }

    #[test]
    fn test_diagnostic_report_children() {
        let child = DiagnosticReport::info(ErrorCode::E0401, "variable shadows outer binding");
        let parent = DiagnosticReport::error(ErrorCode::E0400, "undefined variable")
            .with_child(child);

        assert_eq!(parent.children.len(), 1);
    }

    #[test]
    fn test_legacy_diagnostic_conversion() {
        let diag = Diagnostic::error("test error")
            .with_code("E0001")
            .with_note("help note");

        let report = diag.to_report();
        assert_eq!(report.code, ErrorCode::E0001);
        assert!(report.notes.contains(&"help note".to_string()));
    }

    #[test]
    fn test_diagnostic_from_type_error() {
        use crate::types::OptionPath;
        use super::super::TypeError;

        let err = TypeError::NoDefinition {
            path: OptionPath::new(vec!["services".into(), "nginx".into()]),
        };

        let diag: Diagnostic = err.into();
        assert_eq!(diag.severity, Severity::Error);
        assert!(diag.message.contains("services.nginx"));
    }

    #[test]
    fn test_referenced_files() {
        let span1 = SourceSpan::new(PathBuf::from("a.nix"), 0, 10, 1, 1);
        let span2 = SourceSpan::new(PathBuf::from("b.nix"), 0, 10, 1, 1);

        let report = DiagnosticReport::error(ErrorCode::E0003, "conflict")
            .with_span(span1.clone())
            .with_secondary_label(span2.clone(), "other location");

        let files = report.referenced_files();
        assert_eq!(files.len(), 2);
    }
}
