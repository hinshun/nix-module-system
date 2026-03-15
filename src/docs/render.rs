//! Documentation rendering in multiple formats.

use super::{DocTree, OptionEntry};
use crate::types::{OptionPath, Value};
use std::fmt::Write;

/// Output format for documentation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Markdown format
    Markdown,
    /// HTML format
    Html,
    /// JSON format for programmatic access
    Json,
}

/// Documentation renderer
pub struct DocRenderer {
    format: OutputFormat,
}

impl DocRenderer {
    /// Create a new renderer
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Render the entire documentation tree
    pub fn render(&self, tree: &DocTree) -> String {
        match self.format {
            OutputFormat::Markdown => self.render_markdown(tree),
            OutputFormat::Html => self.render_html(tree),
            OutputFormat::Json => self.render_json(tree),
        }
    }

    /// Render a single option
    pub fn render_option(&self, path: &OptionPath, entry: &OptionEntry) -> String {
        match self.format {
            OutputFormat::Markdown => self.render_option_markdown(path, entry),
            OutputFormat::Html => self.render_option_html(path, entry),
            OutputFormat::Json => self.render_option_json(path, entry),
        }
    }

    fn render_markdown(&self, tree: &DocTree) -> String {
        let mut output = String::new();

        // Title
        writeln!(output, "# {}", tree.metadata.name).unwrap();
        if !tree.metadata.description.is_empty() {
            writeln!(output, "\n{}\n", tree.metadata.description).unwrap();
        }

        // Table of contents
        writeln!(output, "## Options\n").unwrap();

        let top_level = tree.top_level();
        for path in &top_level {
            writeln!(output, "- [`{}`](#{})", path, path.to_dotted().replace('.', "-")).unwrap();
        }

        writeln!(output).unwrap();

        // Option documentation
        for (path, entry) in &tree.options {
            output.push_str(&self.render_option_markdown(path, entry));
            output.push('\n');
        }

        output
    }

    fn render_option_markdown(&self, path: &OptionPath, entry: &OptionEntry) -> String {
        let mut output = String::new();
        let depth = path.components().len();
        let heading = "#".repeat(depth.min(4) + 1);

        // Option heading
        writeln!(output, "{} `{}`", heading, path).unwrap();
        writeln!(output).unwrap();

        // Type
        writeln!(output, "**Type:** `{}`", entry.doc.type_desc).unwrap();
        writeln!(output).unwrap();

        // Default
        if let Some(ref default) = entry.doc.default {
            writeln!(output, "**Default:**").unwrap();
            writeln!(output, "```nix").unwrap();
            writeln!(output, "{}", format_value(default)).unwrap();
            writeln!(output, "```").unwrap();
            writeln!(output).unwrap();
        }

        // Description
        if let Some(ref desc) = entry.doc.description {
            writeln!(output, "{}", desc).unwrap();
            writeln!(output).unwrap();
        }

        // Example
        if let Some(ref example) = entry.doc.example {
            writeln!(output, "**Example:**").unwrap();
            writeln!(output, "```nix").unwrap();
            writeln!(output, "{}", format_value(example)).unwrap();
            writeln!(output, "```").unwrap();
            writeln!(output).unwrap();
        }

        // Related options
        if !entry.related.is_empty() {
            writeln!(output, "**Related:**").unwrap();
            for related in &entry.related {
                writeln!(output, "- [`{}`](#{})", related, related.to_dotted().replace('.', "-"))
                    .unwrap();
            }
            writeln!(output).unwrap();
        }

        output
    }

    fn render_html(&self, tree: &DocTree) -> String {
        let mut output = String::new();

        output.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        writeln!(output, "<title>{}</title>", tree.metadata.name).unwrap();
        output.push_str(include_str!("styles.css"));
        output.push_str("</head>\n<body>\n");

        writeln!(output, "<h1>{}</h1>", tree.metadata.name).unwrap();

        if !tree.metadata.description.is_empty() {
            writeln!(output, "<p class=\"description\">{}</p>", tree.metadata.description).unwrap();
        }

        // Navigation sidebar
        output.push_str("<nav class=\"sidebar\">\n<h2>Options</h2>\n<ul>\n");
        for path in tree.top_level() {
            writeln!(
                output,
                "<li><a href=\"#{}\">{}</a></li>",
                path.to_dotted().replace('.', "-"),
                path
            )
            .unwrap();
        }
        output.push_str("</ul>\n</nav>\n");

        // Main content
        output.push_str("<main>\n");
        for (path, entry) in &tree.options {
            output.push_str(&self.render_option_html(path, entry));
        }
        output.push_str("</main>\n");

        output.push_str("</body>\n</html>");
        output
    }

    fn render_option_html(&self, path: &OptionPath, entry: &OptionEntry) -> String {
        let mut output = String::new();
        let id = path.to_dotted().replace('.', "-");

        writeln!(output, "<section class=\"option\" id=\"{}\">", id).unwrap();
        writeln!(output, "<h3><code>{}</code></h3>", path).unwrap();

        writeln!(
            output,
            "<p class=\"type\"><strong>Type:</strong> <code>{}</code></p>",
            entry.doc.type_desc
        )
        .unwrap();

        if let Some(ref default) = entry.doc.default {
            writeln!(output, "<div class=\"default\">").unwrap();
            writeln!(output, "<strong>Default:</strong>").unwrap();
            writeln!(output, "<pre><code>{}</code></pre>", format_value(default)).unwrap();
            writeln!(output, "</div>").unwrap();
        }

        if let Some(ref desc) = entry.doc.description {
            writeln!(output, "<p class=\"desc\">{}</p>", desc).unwrap();
        }

        if let Some(ref example) = entry.doc.example {
            writeln!(output, "<div class=\"example\">").unwrap();
            writeln!(output, "<strong>Example:</strong>").unwrap();
            writeln!(output, "<pre><code>{}</code></pre>", format_value(example)).unwrap();
            writeln!(output, "</div>").unwrap();
        }

        writeln!(output, "</section>").unwrap();
        output
    }

    fn render_json(&self, tree: &DocTree) -> String {
        let mut entries = Vec::new();

        for (path, entry) in &tree.options {
            entries.push(serde_json::json!({
                "path": path.to_dotted(),
                "type": entry.doc.type_desc,
                "default": entry.doc.default.as_ref().map(|v| format!("{}", v)),
                "description": entry.doc.description,
                "example": entry.doc.example.as_ref().map(|v| format!("{}", v)),
                "internal": entry.doc.internal,
                "visible": entry.doc.visible,
                "readOnly": entry.doc.read_only,
                "children": entry.children.iter().map(|p| p.to_dotted()).collect::<Vec<_>>(),
                "related": entry.related.iter().map(|p| p.to_dotted()).collect::<Vec<_>>(),
            }));
        }

        serde_json::json!({
            "metadata": {
                "name": tree.metadata.name,
                "version": tree.metadata.version,
                "description": tree.metadata.description,
                "authors": tree.metadata.authors,
            },
            "options": entries
        })
        .to_string()
    }

    fn render_option_json(&self, path: &OptionPath, entry: &OptionEntry) -> String {
        serde_json::json!({
            "path": path.to_dotted(),
            "type": entry.doc.type_desc,
            "default": entry.doc.default.as_ref().map(|v| format!("{}", v)),
            "description": entry.doc.description,
            "example": entry.doc.example.as_ref().map(|v| format!("{}", v)),
        })
        .to_string()
    }
}

/// Format a value for display
fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("\"{}\"", s.escape_default()),
        Value::Path(p) => p.display().to_string(),
        Value::List(items) => {
            let items_str: Vec<String> = items.iter().map(format_value).collect();
            format!("[ {} ]", items_str.join(" "))
        }
        Value::Attrs(attrs) => {
            let pairs: Vec<String> = attrs
                .iter()
                .map(|(k, v)| format!("{} = {};", k, format_value(v)))
                .collect();
            format!("{{ {} }}", pairs.join(" "))
        }
        Value::Lambda => "<function>".to_string(),
        Value::Derivation(_) => "<derivation>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docs::DocMetadata;

    #[test]
    fn test_markdown_render() {
        let mut tree = DocTree::new();
        tree.metadata = DocMetadata {
            name: "Test Module".to_string(),
            version: "1.0.0".to_string(),
            description: "A test module".to_string(),
            authors: vec!["Test Author".to_string()],
        };

        let renderer = DocRenderer::new(OutputFormat::Markdown);
        let output = renderer.render(&tree);

        assert!(output.contains("# Test Module"));
        assert!(output.contains("A test module"));
    }

    #[test]
    fn test_json_render() {
        let mut tree = DocTree::new();
        tree.metadata.name = "Test".to_string();

        let renderer = DocRenderer::new(OutputFormat::Json);
        let output = renderer.render(&tree);

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["metadata"]["name"], "Test");
    }
}
