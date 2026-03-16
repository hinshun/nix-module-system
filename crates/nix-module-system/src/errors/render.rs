//! Error rendering with ariadne for beautiful terminal output.
//!
//! This module provides functions to render diagnostics using the ariadne crate,
//! producing colorful, informative error messages with source context.
//!
//! # Features
//!
//! - Multi-span error rendering with primary and secondary labels
//! - Multi-file errors (e.g., conflicting definitions across files)
//! - Colorful terminal output with proper ANSI colors
//! - Source code context with line numbers
//! - Help notes and suggestions
//! - Automatic source file loading with caching
//!
//! # Example
//!
//! ```ignore
//! use nix_module_system::errors::{
//!     render_to_terminal, render_nix_error, SourceCache,
//!     DiagnosticReport, ErrorCode, SourceSpan, NixError,
//! };
//! use std::path::PathBuf;
//!
//! // Create a source cache and add file contents
//! let mut cache = SourceCache::new();
//! cache.add(PathBuf::from("config.nix"), source_code.to_string());
//!
//! // Render a diagnostic report
//! let span = SourceSpan::new(PathBuf::from("config.nix"), 10, 20, 5, 3);
//! let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch")
//!     .with_primary_label(span, "expected bool, found string")
//!     .with_help("use `true` or `false` for boolean values");
//!
//! let output = render_to_terminal(&report, &mut cache);
//! eprintln!("{}", output);
//!
//! // Or render a NixError directly
//! let err = NixError::TypeMismatch {
//!     expected: "bool".to_string(),
//!     found: "string".to_string(),
//!     span,
//!     context: None,
//! };
//! let output = render_nix_error(&err, &mut cache);
//! eprintln!("{}", output);
//! ```

use super::{Diagnostic, DiagnosticReport, LabelStyle, NixError, Severity, SourceLocation};
use ariadne::{Color, ColorGenerator, Config, Label, Report, ReportKind, Source};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

/// Cache for source files.
///
/// Stores file contents for rendering error messages with source context.
pub struct SourceCache {
    sources: HashMap<PathBuf, String>,
}

impl SourceCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
        }
    }

    /// Add a source file to the cache.
    pub fn add(&mut self, path: PathBuf, content: String) {
        self.sources.insert(path, content);
    }

    /// Get a source file, loading from disk if not cached.
    pub fn get_or_load(&mut self, path: &PathBuf) -> Option<&str> {
        if !self.sources.contains_key(path) {
            if let Ok(content) = std::fs::read_to_string(path) {
                self.sources.insert(path.clone(), content);
            }
        }
        self.sources.get(path).map(|s| s.as_str())
    }

    /// Check if a file is in the cache.
    pub fn contains(&self, path: &PathBuf) -> bool {
        self.sources.contains_key(path)
    }

    /// Get a source file from the cache (doesn't load from disk).
    pub fn get(&self, path: &PathBuf) -> Option<&str> {
        self.sources.get(path).map(|s| s.as_str())
    }

    /// Clear the cache.
    pub fn clear(&mut self) {
        self.sources.clear();
    }
}

impl Default for SourceCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a DiagnosticReport to a string using ariadne.
pub fn render_report(report: &DiagnosticReport, cache: &mut SourceCache) -> String {
    let mut output = Vec::new();
    render_report_to(report, cache, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

/// Render a DiagnosticReport to a string for terminal display.
///
/// This is the main entry point for rendering errors to the terminal with
/// full color support using ariadne. The function:
/// - Uses the SourceCache to load source file contents
/// - Renders multi-span errors with related locations
/// - Produces colorful, informative output
///
/// # Example
///
/// ```ignore
/// use nix_module_system::errors::{DiagnosticReport, ErrorCode, SourceSpan, SourceCache};
///
/// let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch")
///     .with_primary_label(span, "expected bool");
///
/// let mut cache = SourceCache::new();
/// let output = render_to_terminal(&report, &mut cache);
/// eprintln!("{}", output);
/// ```
pub fn render_to_terminal(report: &DiagnosticReport, sources: &mut SourceCache) -> String {
    render_report(report, sources)
}

/// Render a DiagnosticReport to a writer using ariadne.
pub fn render_report_to<W: Write>(
    report: &DiagnosticReport,
    cache: &mut SourceCache,
    writer: &mut W,
) -> std::io::Result<()> {
    // Get primary span for positioning
    let primary_span = report.primary_or_first_span();

    if let Some(primary) = primary_span {
        render_report_with_ariadne(report, primary, cache, writer)
    } else {
        render_report_simple(report, writer)
    }
}

/// Render a DiagnosticReport with ariadne (has source location).
fn render_report_with_ariadne<W: Write>(
    report: &DiagnosticReport,
    primary: &super::SourceSpan,
    cache: &mut SourceCache,
    writer: &mut W,
) -> std::io::Result<()> {
    let kind = match report.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Info => ReportKind::Advice,
        Severity::Hint => ReportKind::Advice,
    };

    let file_id = primary.file.display().to_string();

    let mut colors = ColorGenerator::new();
    let primary_color = match report.severity {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Info => Color::Blue,
        Severity::Hint => Color::Cyan,
    };

    // Build the ariadne report
    let mut ariadne_report = Report::build(kind, file_id.clone(), primary.start)
        .with_message(&report.message)
        .with_config(Config::default().with_color(true))
        .with_code(format!("{}", report.code));

    // Add labels
    for label in &report.labels {
        let label_file = label.span.file.display().to_string();
        let color = match label.style {
            LabelStyle::Primary => primary_color,
            LabelStyle::Secondary => colors.next(),
            LabelStyle::Help => Color::Green,
            LabelStyle::Note => Color::Blue,
        };

        ariadne_report = ariadne_report.with_label(
            Label::new((label_file, label.span.start..label.span.end))
                .with_message(&label.message)
                .with_color(color),
        );
    }

    // If no labels but we have a primary span, add it
    if report.labels.is_empty() {
        if let Some(span) = &report.primary_span {
            ariadne_report = ariadne_report.with_label(
                Label::new((file_id.clone(), span.start..span.end))
                    .with_message(&report.message)
                    .with_color(primary_color),
            );
        }
    }

    // Add related locations as secondary labels
    for related in &report.related {
        let related_file = related.span.file.display().to_string();
        ariadne_report = ariadne_report.with_label(
            Label::new((related_file, related.span.start..related.span.end))
                .with_message(&related.message)
                .with_color(colors.next()),
        );
    }

    // Add notes
    for note in &report.notes {
        ariadne_report = ariadne_report.with_note(note);
    }

    // Add suggestions as notes (ariadne doesn't have native suggestion support)
    for suggestion in &report.suggestions {
        let note = if suggestion.replacement.is_some() {
            format!("suggestion: {} (can be auto-applied)", suggestion.message)
        } else {
            format!("help: {}", suggestion.message)
        };
        ariadne_report = ariadne_report.with_note(note);
    }

    // Collect all files we need
    let mut file_sources: HashMap<String, Source<String>> = HashMap::new();

    // Add primary file
    if let Some(content) = cache.get_or_load(&primary.file) {
        file_sources.insert(file_id.clone(), Source::from(content.to_string()));
    }

    // Add label files
    for label in &report.labels {
        let label_file_id = label.span.file.display().to_string();
        if !file_sources.contains_key(&label_file_id) {
            if let Some(content) = cache.get_or_load(&label.span.file) {
                file_sources.insert(label_file_id, Source::from(content.to_string()));
            }
        }
    }

    // Add related location files
    for related in &report.related {
        let related_file_id = related.span.file.display().to_string();
        if !file_sources.contains_key(&related_file_id) {
            if let Some(content) = cache.get_or_load(&related.span.file) {
                file_sources.insert(related_file_id, Source::from(content.to_string()));
            }
        }
    }

    // Create cache adapter and write
    let ariadne_cache = AriadneCache {
        sources: file_sources,
    };

    ariadne_report
        .finish()
        .write(&ariadne_cache, &mut *writer)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Render children recursively
    for child in &report.children {
        writeln!(writer)?;
        render_report_to(child, cache, writer)?;
    }

    Ok(())
}

/// Render a DiagnosticReport without source location.
fn render_report_simple<W: Write>(report: &DiagnosticReport, writer: &mut W) -> std::io::Result<()> {
    let prefix = match report.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    };

    writeln!(writer, "{}[{}]: {}", prefix, report.code, report.message)?;

    for note in &report.notes {
        writeln!(writer, "  = help: {}", note)?;
    }

    for suggestion in &report.suggestions {
        writeln!(writer, "  = suggestion: {}", suggestion.message)?;
    }

    // Render children
    for child in &report.children {
        writeln!(writer)?;
        render_report_simple(child, writer)?;
    }

    Ok(())
}

/// Render a NixError to a string.
pub fn render_nix_error(err: &NixError, cache: &mut SourceCache) -> String {
    render_report(&err.to_diagnostic(), cache)
}

/// Render a NixError to a writer.
pub fn render_nix_error_to<W: Write>(
    err: &NixError,
    cache: &mut SourceCache,
    writer: &mut W,
) -> std::io::Result<()> {
    render_report_to(&err.to_diagnostic(), cache, writer)
}

// ========== Legacy API compatibility ==========

/// Render a legacy Diagnostic to a string.
pub fn render_diagnostic(diag: &Diagnostic, cache: &mut SourceCache) -> String {
    let mut output = Vec::new();
    render_diagnostic_to(diag, cache, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

/// Render a legacy Diagnostic to a writer.
pub fn render_diagnostic_to<W: Write>(
    diag: &Diagnostic,
    cache: &mut SourceCache,
    writer: &mut W,
) -> std::io::Result<()> {
    // If we have a primary location, use ariadne
    if let Some(primary) = &diag.primary {
        render_with_ariadne(diag, primary, cache, writer)
    } else {
        // Simple text output without location
        render_simple(diag, writer)
    }
}

/// Render using ariadne with source code context.
fn render_with_ariadne<W: Write>(
    diag: &Diagnostic,
    primary: &SourceLocation,
    cache: &mut SourceCache,
    writer: &mut W,
) -> std::io::Result<()> {
    let kind = match diag.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Info => ReportKind::Advice,
        Severity::Hint => ReportKind::Advice,
    };

    let file_id = primary.file.display().to_string();

    let mut colors = ColorGenerator::new();
    let primary_color = match diag.severity {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Info => Color::Blue,
        Severity::Hint => Color::Cyan,
    };

    // Build the report
    let mut report = Report::build(kind, file_id.clone(), primary.start)
        .with_message(&diag.message)
        .with_config(Config::default().with_color(true));

    // Add error code if present
    if let Some(code) = &diag.code {
        report = report.with_code(code);
    }

    // Add primary label
    report = report.with_label(
        Label::new((file_id.clone(), primary.start..primary.end))
            .with_message(&diag.message)
            .with_color(primary_color),
    );

    // Add secondary labels
    for (loc, msg) in &diag.secondary {
        let secondary_file = loc.file.display().to_string();
        report = report.with_label(
            Label::new((secondary_file, loc.start..loc.end))
                .with_message(msg)
                .with_color(colors.next()),
        );
    }

    // Add notes
    for note in &diag.notes {
        report = report.with_note(note);
    }

    // Collect all files we need
    let mut file_sources: HashMap<String, Source<String>> = HashMap::new();

    // Add primary file
    if let Some(content) = cache.get_or_load(&primary.file) {
        file_sources.insert(file_id.clone(), Source::from(content.to_string()));
    }

    // Add secondary files
    for (loc, _) in &diag.secondary {
        let file_id = loc.file.display().to_string();
        if !file_sources.contains_key(&file_id) {
            if let Some(content) = cache.get_or_load(&loc.file) {
                file_sources.insert(file_id, Source::from(content.to_string()));
            }
        }
    }

    // Create a cache adapter for ariadne
    let ariadne_cache = AriadneCache {
        sources: file_sources,
    };

    // Write the report
    report
        .finish()
        .write(&ariadne_cache, writer)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

/// Render without source location.
fn render_simple<W: Write>(diag: &Diagnostic, writer: &mut W) -> std::io::Result<()> {
    let prefix = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    };

    if let Some(code) = &diag.code {
        writeln!(writer, "{}[{}]: {}", prefix, code, diag.message)?;
    } else {
        writeln!(writer, "{}: {}", prefix, diag.message)?;
    }

    for note in &diag.notes {
        writeln!(writer, "  = help: {}", note)?;
    }

    Ok(())
}

/// Adapter to implement ariadne's Cache trait.
struct AriadneCache {
    sources: HashMap<String, Source<String>>,
}

impl ariadne::Cache<String> for &AriadneCache {
    type Storage = String;

    fn fetch(
        &mut self,
        id: &String,
    ) -> Result<&Source<Self::Storage>, Box<dyn std::fmt::Debug + '_>> {
        self.sources.get(id).ok_or_else(|| {
            Box::new(format!("file not found: {}", id)) as Box<dyn std::fmt::Debug>
        })
    }

    fn display<'a>(&self, id: &'a String) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(id.clone()))
    }
}

/// Format a Nix-style error message (for compatibility with Nix output).
pub fn format_nix_error(diag: &Diagnostic) -> String {
    let mut result = String::new();

    let prefix = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "info",
    };

    result.push_str(&format!("{}: {}\n", prefix, diag.message));

    if let Some(primary) = &diag.primary {
        result.push_str(&format!(
            "       at {}:{}:{}\n",
            primary.file.display(),
            primary.line,
            primary.column
        ));
    }

    for (loc, msg) in &diag.secondary {
        result.push_str(&format!(
            "       ... {}:{}:{}: {}\n",
            loc.file.display(),
            loc.line,
            loc.column,
            msg
        ));
    }

    for note in &diag.notes {
        result.push_str(&format!("       = help: {}\n", note));
    }

    result
}

/// Format a DiagnosticReport in Nix-style.
pub fn format_nix_report(report: &DiagnosticReport) -> String {
    let mut result = String::new();

    let prefix = match report.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        _ => "info",
    };

    result.push_str(&format!("{}[{}]: {}\n", prefix, report.code, report.message));

    if let Some(span) = &report.primary_span {
        result.push_str(&format!(
            "       at {}:{}:{}\n",
            span.file.display(),
            span.line,
            span.column
        ));
    }

    for label in &report.labels {
        if label.style != LabelStyle::Primary {
            result.push_str(&format!(
                "       ... {}:{}:{}: {}\n",
                label.span.file.display(),
                label.span.line,
                label.span.column,
                label.message
            ));
        }
    }

    for related in &report.related {
        result.push_str(&format!(
            "       ... {}:{}:{}: {}\n",
            related.span.file.display(),
            related.span.line,
            related.span.column,
            related.message
        ));
    }

    for note in &report.notes {
        result.push_str(&format!("       = help: {}\n", note));
    }

    for suggestion in &report.suggestions {
        result.push_str(&format!("       = suggestion: {}\n", suggestion.message));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::{ErrorCode, SourceSpan};
    use std::path::PathBuf;

    #[test]
    fn test_simple_render() {
        let diag = Diagnostic::error("something went wrong").with_code("E0001");

        let mut output = Vec::new();
        render_simple(&diag, &mut output).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("error[E0001]"));
        assert!(result.contains("something went wrong"));
    }

    #[test]
    fn test_source_cache() {
        let mut cache = SourceCache::new();
        cache.add(PathBuf::from("test.nix"), "{ }".to_string());

        assert!(cache.get_or_load(&PathBuf::from("test.nix")).is_some());
        assert!(cache.contains(&PathBuf::from("test.nix")));
    }

    #[test]
    fn test_nix_format() {
        let diag = Diagnostic::error("type mismatch").with_note("expected bool");

        let result = format_nix_error(&diag);
        assert!(result.contains("error: type mismatch"));
        assert!(result.contains("help: expected bool"));
    }

    #[test]
    fn test_render_report_simple() {
        let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch")
            .with_note("expected bool");

        let mut output = Vec::new();
        render_report_simple(&report, &mut output).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("error[E0001]"));
        assert!(result.contains("type mismatch"));
        assert!(result.contains("help: expected bool"));
    }

    #[test]
    fn test_render_nix_error() {
        let err = NixError::UndefinedVariable {
            name: "foo".to_string(),
            span: SourceSpan::virtual_span("test"),
            similar: vec![],
        };

        let mut cache = SourceCache::new();
        let output = render_nix_error(&err, &mut cache);

        assert!(output.contains("undefined variable"));
        assert!(output.contains("foo"));
    }

    #[test]
    fn test_format_nix_report() {
        let report = DiagnosticReport::error(ErrorCode::E0001, "type mismatch")
            .with_span(SourceSpan::new(PathBuf::from("test.nix"), 10, 20, 5, 3))
            .with_note("expected bool");

        let result = format_nix_report(&report);
        assert!(result.contains("error[E0001]: type mismatch"));
        assert!(result.contains("at test.nix:5:3"));
        assert!(result.contains("help: expected bool"));
    }

    #[test]
    fn test_source_cache_clear() {
        let mut cache = SourceCache::new();
        cache.add(PathBuf::from("test.nix"), "{ }".to_string());
        assert!(cache.contains(&PathBuf::from("test.nix")));

        cache.clear();
        assert!(!cache.contains(&PathBuf::from("test.nix")));
    }

    // ========== Real Error Scenario Tests ==========

    #[test]
    fn test_render_type_mismatch_error() {
        // Simulate a type mismatch error with source context
        let source = r#"{
  services.nginx.enable = "yes";
}"#;
        let mut cache = SourceCache::new();
        cache.add(PathBuf::from("config.nix"), source.to_string());

        let span = SourceSpan::new(
            PathBuf::from("config.nix"),
            28,  // start of "yes"
            33,  // end of "yes"
            2,
            28,
        );

        let err = NixError::TypeMismatch {
            expected: "bool".to_string(),
            found: "string".to_string(),
            span: span.clone(),
            context: Some("in option services.nginx.enable".to_string()),
        };

        let output = render_nix_error(&err, &mut cache);

        // Verify the output contains expected elements
        assert!(output.contains("expected bool"));
        assert!(output.contains("string"));
        // The output should reference the file
        assert!(output.contains("config.nix"));
    }

    #[test]
    fn test_render_parse_error_with_source_context() {
        // Simulate a parse error
        let source = r#"{
  foo = bar
  baz = 1;
}"#;
        let mut cache = SourceCache::new();
        cache.add(PathBuf::from("invalid.nix"), source.to_string());

        let span = SourceSpan::new(
            PathBuf::from("invalid.nix"),
            14,  // after "bar"
            14,
            2,
            13,
        );

        let err = NixError::ParseError {
            message: "expected `;` after attribute".to_string(),
            span: span.clone(),
            hints: vec!["add a semicolon after the value".to_string()],
        };

        let output = render_nix_error(&err, &mut cache);

        // Verify the output contains expected elements
        assert!(output.contains("expected"));
        assert!(output.contains("semicolon") || output.contains(";"));
        assert!(output.contains("invalid.nix"));
    }

    #[test]
    fn test_render_module_error_with_multiple_locations() {
        // Simulate a conflicting definitions error with multiple locations
        let source_a = r#"{
  services.nginx.enable = true;
}"#;
        let source_b = r#"{
  services.nginx.enable = false;
}"#;
        let mut cache = SourceCache::new();
        cache.add(PathBuf::from("a.nix"), source_a.to_string());
        cache.add(PathBuf::from("b.nix"), source_b.to_string());

        use crate::errors::RelatedLocation;

        let span_a = SourceSpan::new(PathBuf::from("a.nix"), 28, 32, 2, 28);
        let span_b = SourceSpan::new(PathBuf::from("b.nix"), 28, 33, 2, 28);

        let err = NixError::ConflictingDefinitions {
            path: "services.nginx.enable".to_string(),
            definitions: vec![
                RelatedLocation::new(span_a, "first definition"),
                RelatedLocation::new(span_b, "conflicting definition"),
            ],
            values: vec!["true".to_string(), "false".to_string()],
        };

        let output = render_nix_error(&err, &mut cache);

        // Verify multi-file error handling
        assert!(output.contains("conflicting"));
        assert!(output.contains("services.nginx.enable"));
    }

    #[test]
    fn test_render_undefined_variable_with_suggestions() {
        let source = r#"let
  enableNginx = true;
in {
  services.nginx.enable = enableNginxs;
}"#;
        let mut cache = SourceCache::new();
        cache.add(PathBuf::from("config.nix"), source.to_string());

        let span = SourceSpan::new(PathBuf::from("config.nix"), 55, 67, 4, 28);

        let err = NixError::UndefinedVariable {
            name: "enableNginxs".to_string(),
            span: span.clone(),
            similar: vec!["enableNginx".to_string()],
        };

        let output = render_nix_error(&err, &mut cache);

        // Verify undefined variable error with suggestion
        assert!(output.contains("undefined variable"));
        assert!(output.contains("enableNginxs"));
        // Should suggest the similar variable
        assert!(output.contains("enableNginx") || output.contains("did you mean"));
    }

    #[test]
    fn test_render_to_terminal_api() {
        // Test the render_to_terminal function
        let report = DiagnosticReport::error(ErrorCode::E0001, "test error message")
            .with_note("this is a helpful note");

        let mut cache = SourceCache::new();
        let output = render_to_terminal(&report, &mut cache);

        assert!(output.contains("test error message"));
        assert!(output.contains("helpful note"));
    }

    #[test]
    fn test_render_multispan_with_ariadne() {
        // Test rendering with actual ariadne output for multi-span errors
        let source = r#"{
  option1 = "value1";
  option2 = "value2";
}"#;
        let mut cache = SourceCache::new();
        cache.add(PathBuf::from("test.nix"), source.to_string());

        let primary_span = SourceSpan::new(PathBuf::from("test.nix"), 14, 22, 2, 13);
        let secondary_span = SourceSpan::new(PathBuf::from("test.nix"), 38, 46, 3, 13);

        let report = DiagnosticReport::error(ErrorCode::E0003, "conflicting values")
            .with_primary_label(primary_span, "first value here")
            .with_secondary_label(secondary_span, "conflicts with this value")
            .with_help("use mkForce to override");

        let output = render_to_terminal(&report, &mut cache);

        // The ariadne output should contain the diagnostic info
        assert!(output.contains("conflicting"));
        assert!(output.contains("test.nix"));
    }

    #[test]
    fn test_render_error_without_source() {
        // Test rendering when source file is not in cache
        let span = SourceSpan::new(PathBuf::from("nonexistent.nix"), 10, 20, 5, 3);
        let report = DiagnosticReport::error(ErrorCode::E0001, "error in missing file")
            .with_primary_label(span, "problem here");

        let mut cache = SourceCache::new();
        // Don't add the file to cache - should still render gracefully
        let output = render_to_terminal(&report, &mut cache);

        // Should still produce output even without source
        assert!(output.contains("error") || output.contains("missing"));
    }

    #[test]
    fn test_render_child_diagnostics() {
        // Test rendering diagnostics with child diagnostics
        let child = DiagnosticReport::info(ErrorCode::E0401, "variable shadows outer binding");
        let parent = DiagnosticReport::error(ErrorCode::E0400, "evaluation failed")
            .with_child(child)
            .with_note("check variable bindings");

        let mut cache = SourceCache::new();
        let output = render_to_terminal(&parent, &mut cache);

        // Both parent and child messages should appear
        assert!(output.contains("evaluation failed") || output.contains("E0400"));
    }

    #[test]
    fn test_render_all_severity_levels() {
        let mut cache = SourceCache::new();

        // Test error
        let error = DiagnosticReport::error(ErrorCode::E0001, "this is an error");
        let output = render_to_terminal(&error, &mut cache);
        assert!(output.contains("error") || output.contains("Error"));

        // Test warning
        let warning = DiagnosticReport::warning(ErrorCode::E0401, "this is a warning");
        let output = render_to_terminal(&warning, &mut cache);
        assert!(output.contains("warning") || output.contains("Warning"));

        // Test info
        let info = DiagnosticReport::info(ErrorCode::E0401, "this is info");
        let output = render_to_terminal(&info, &mut cache);
        assert!(!output.is_empty());

        // Test hint
        let hint = DiagnosticReport::hint(ErrorCode::E0401, "this is a hint");
        let output = render_to_terminal(&hint, &mut cache);
        assert!(!output.is_empty());
    }
}
