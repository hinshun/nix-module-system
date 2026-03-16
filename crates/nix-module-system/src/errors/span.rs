//! Source span types for error reporting.
//!
//! This module provides types for representing locations in source code,
//! compatible with ariadne rendering and JSON serialization.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

/// A span in source code with full location information.
///
/// This is the primary type for source-mapping errors. It contains:
/// - File path (for multi-file errors)
/// - Byte range (for ariadne integration)
/// - Line/column (for human-readable output)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Source file path (can be virtual for REPL/stdin)
    pub file: PathBuf,

    /// Start byte offset (0-indexed)
    pub start: usize,

    /// End byte offset (exclusive, 0-indexed)
    pub end: usize,

    /// Start line number (1-indexed for human readability)
    pub line: usize,

    /// Start column number (1-indexed for human readability)
    pub column: usize,

    /// End line number (1-indexed), for multi-line spans
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,

    /// End column number (1-indexed), for multi-line spans
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

impl SourceSpan {
    /// Create a new source span.
    ///
    /// For single-line spans, end_line and end_column can be None.
    pub fn new(
        file: PathBuf,
        start: usize,
        end: usize,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            file,
            start,
            end,
            line,
            column,
            end_line: None,
            end_column: None,
        }
    }

    /// Create a span with full multi-line information.
    pub fn new_multiline(
        file: PathBuf,
        start: usize,
        end: usize,
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
    ) -> Self {
        Self {
            file,
            start,
            end,
            line,
            column,
            end_line: Some(end_line),
            end_column: Some(end_column),
        }
    }

    /// Create a point span (zero-width, for insertion points).
    pub fn point(file: PathBuf, offset: usize, line: usize, column: usize) -> Self {
        Self {
            file,
            start: offset,
            end: offset,
            line,
            column,
            end_line: None,
            end_column: None,
        }
    }

    /// Create a span for an entire file (useful for file-level errors).
    pub fn entire_file(file: PathBuf) -> Self {
        Self {
            file,
            start: 0,
            end: 0,
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
        }
    }

    /// Create a virtual span for generated code or REPL input.
    pub fn virtual_span(name: &str) -> Self {
        Self {
            file: PathBuf::from(format!("<{}>", name)),
            start: 0,
            end: 0,
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
        }
    }

    /// Get the byte range for ariadne.
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    /// Get the length of the span in bytes.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Check if the span is empty (zero-width).
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Check if this span is from a virtual source.
    pub fn is_virtual(&self) -> bool {
        self.file
            .to_string_lossy()
            .starts_with('<')
            && self.file.to_string_lossy().ends_with('>')
    }

    /// Extend this span to include another span.
    ///
    /// The resulting span covers both original spans.
    pub fn extend(&self, other: &SourceSpan) -> Self {
        debug_assert_eq!(
            self.file, other.file,
            "Cannot extend span across different files"
        );

        let (start, line, column) = if self.start <= other.start {
            (self.start, self.line, self.column)
        } else {
            (other.start, other.line, other.column)
        };

        let (end, end_line, end_column) = if self.end >= other.end {
            (self.end, self.end_line, self.end_column)
        } else {
            (other.end, other.end_line, other.end_column)
        };

        Self {
            file: self.file.clone(),
            start,
            end,
            line,
            column,
            end_line,
            end_column,
        }
    }

    /// Create a span from our parser's Span type.
    pub fn from_parser_span(span: &crate::types::Span) -> Self {
        Self {
            file: span.file.clone(),
            start: span.start,
            end: span.end,
            line: span.line,
            column: span.column,
            end_line: None,
            end_column: None,
        }
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.file.display(),
            self.line,
            self.column
        )
    }
}

impl From<crate::types::Span> for SourceSpan {
    fn from(span: crate::types::Span) -> Self {
        Self {
            file: span.file,
            start: span.start,
            end: span.end,
            line: span.line,
            column: span.column,
            end_line: None,
            end_column: None,
        }
    }
}

impl From<&crate::types::Span> for SourceSpan {
    fn from(span: &crate::types::Span) -> Self {
        Self {
            file: span.file.clone(),
            start: span.start,
            end: span.end,
            line: span.line,
            column: span.column,
            end_line: None,
            end_column: None,
        }
    }
}

/// A labeled span for error reporting.
///
/// Labels attach messages to specific source locations with styling hints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// The source span this label points to
    pub span: SourceSpan,

    /// The message to display at this location
    pub message: String,

    /// The style of this label (primary, secondary, etc.)
    pub style: LabelStyle,
}

impl Label {
    /// Create a new primary label.
    pub fn primary(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            style: LabelStyle::Primary,
        }
    }

    /// Create a new secondary label.
    pub fn secondary(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            style: LabelStyle::Secondary,
        }
    }

    /// Create a new help label.
    pub fn help(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            style: LabelStyle::Help,
        }
    }

    /// Create a new note label.
    pub fn note(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            style: LabelStyle::Note,
        }
    }

    /// Change the style of this label.
    pub fn with_style(mut self, style: LabelStyle) -> Self {
        self.style = style;
        self
    }
}

/// Style for labels in error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelStyle {
    /// Primary label (usually the main error location)
    Primary,
    /// Secondary label (related locations, context)
    Secondary,
    /// Help label (suggestions, fix hints)
    Help,
    /// Note label (additional information)
    Note,
}

impl Default for LabelStyle {
    fn default() -> Self {
        LabelStyle::Primary
    }
}

/// A related location for multi-span error context.
///
/// This is used to show "defined here" or "previous definition" locations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedLocation {
    /// The source span
    pub span: SourceSpan,

    /// Description of this location's relationship to the error
    pub message: String,
}

impl RelatedLocation {
    /// Create a new related location.
    pub fn new(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    /// Create a "defined here" location.
    pub fn defined_here(span: SourceSpan) -> Self {
        Self::new(span, "defined here")
    }

    /// Create a "previous definition" location.
    pub fn previous_definition(span: SourceSpan) -> Self {
        Self::new(span, "previous definition here")
    }

    /// Create a "declared here" location.
    pub fn declared_here(span: SourceSpan) -> Self {
        Self::new(span, "declared here")
    }

    /// Create a "conflicting definition" location.
    pub fn conflicting_definition(span: SourceSpan) -> Self {
        Self::new(span, "conflicting definition here")
    }

    /// Create an "imported here" location.
    pub fn imported_here(span: SourceSpan) -> Self {
        Self::new(span, "imported here")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_span_new() {
        let span = SourceSpan::new(
            PathBuf::from("test.nix"),
            10,
            20,
            5,
            3,
        );

        assert_eq!(span.file, PathBuf::from("test.nix"));
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
        assert_eq!(span.line, 5);
        assert_eq!(span.column, 3);
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
    }

    #[test]
    fn test_source_span_point() {
        let span = SourceSpan::point(PathBuf::from("test.nix"), 15, 3, 8);
        assert!(span.is_empty());
        assert_eq!(span.len(), 0);
    }

    #[test]
    fn test_source_span_virtual() {
        let span = SourceSpan::virtual_span("repl");
        assert!(span.is_virtual());
        assert_eq!(span.file, PathBuf::from("<repl>"));
    }

    #[test]
    fn test_source_span_extend() {
        let span1 = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let span2 = SourceSpan::new(PathBuf::from("test.nix"), 25, 35, 6, 1);

        let extended = span1.extend(&span2);
        assert_eq!(extended.start, 10);
        assert_eq!(extended.end, 35);
        assert_eq!(extended.line, 5);
    }

    #[test]
    fn test_source_span_display() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        assert_eq!(format!("{}", span), "test.nix:5:3");
    }

    #[test]
    fn test_label_primary() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let label = Label::primary(span.clone(), "type mismatch");

        assert_eq!(label.span, span);
        assert_eq!(label.message, "type mismatch");
        assert_eq!(label.style, LabelStyle::Primary);
    }

    #[test]
    fn test_label_secondary() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let label = Label::secondary(span.clone(), "defined here");

        assert_eq!(label.style, LabelStyle::Secondary);
    }

    #[test]
    fn test_related_location() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let related = RelatedLocation::defined_here(span);

        assert_eq!(related.message, "defined here");
    }

    #[test]
    fn test_source_span_serialization() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let json = serde_json::to_string(&span).unwrap();
        let deserialized: SourceSpan = serde_json::from_str(&json).unwrap();

        assert_eq!(span, deserialized);
    }

    #[test]
    fn test_label_serialization() {
        let span = SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3);
        let label = Label::primary(span, "error message");
        let json = serde_json::to_string(&label).unwrap();
        let deserialized: Label = serde_json::from_str(&json).unwrap();

        assert_eq!(label, deserialized);
    }
}
