//! LSP hover provider.
//!
//! Provides hover information for options, showing types, defaults, and descriptions.

use super::{Capabilities, OptionRegistry};
use crate::types::{OptionPath, Value};
use std::path::PathBuf;

/// Hover information
#[derive(Debug, Clone)]
pub struct HoverInfo {
    /// Option path
    pub path: OptionPath,
    /// Type description
    pub type_desc: String,
    /// Default value (if any)
    pub default: Option<String>,
    /// Description (if any)
    pub description: Option<String>,
    /// Where the option was declared
    pub declared_in: Vec<PathBuf>,
    /// Example value (if any)
    pub example: Option<String>,
}

impl HoverInfo {
    /// Format hover info as markdown
    pub fn to_markdown(&self) -> String {
        let mut lines = Vec::new();

        // Option path as header
        lines.push(format!("### `{}`", self.path));

        // Type
        lines.push(format!("**Type:** `{}`", self.type_desc));

        // Default
        if let Some(ref default) = self.default {
            lines.push(format!("**Default:** `{}`", default));
        }

        // Example
        if let Some(ref example) = self.example {
            lines.push(format!("**Example:** `{}`", example));
        }

        // Description
        if let Some(ref desc) = self.description {
            lines.push(String::new());
            lines.push(desc.clone());
        }

        // Declared in
        if !self.declared_in.is_empty() {
            lines.push(String::new());
            let paths: Vec<_> = self.declared_in.iter().map(|p| p.display().to_string()).collect();
            lines.push(format!(
                "*Declared in:* {}",
                paths.join(", ")
            ));
        }

        lines.join("\n")
    }

    /// Format as plain text (for terminals without markdown support)
    pub fn to_plain_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push(self.path.to_dotted());
        lines.push(format!("Type: {}", self.type_desc));

        if let Some(ref default) = self.default {
            lines.push(format!("Default: {}", default));
        }

        if let Some(ref example) = self.example {
            lines.push(format!("Example: {}", example));
        }

        if let Some(ref desc) = self.description {
            lines.push(String::new());
            lines.push(desc.clone());
        }

        if !self.declared_in.is_empty() {
            lines.push(String::new());
            let paths: Vec<_> = self.declared_in.iter().map(|p| p.display().to_string()).collect();
            lines.push(format!("Declared in: {}", paths.join(", ")));
        }

        lines.join("\n")
    }
}

/// Hover context
#[derive(Debug, Clone)]
pub struct HoverContext {
    /// Position in the document (line)
    pub line: u32,
    /// Column position
    pub column: u32,
    /// Word at cursor position
    pub word: String,
    /// Full line content
    pub line_content: String,
}

impl HoverContext {
    /// Create a new hover context
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            line,
            column,
            word: String::new(),
            line_content: String::new(),
        }
    }

    /// Set the word at cursor
    pub fn with_word(mut self, word: String) -> Self {
        self.word = word;
        self
    }

    /// Set the line content
    pub fn with_line_content(mut self, content: String) -> Self {
        self.line_content = content;
        self
    }

    /// Extract the option path from context
    pub fn extract_path(&self) -> Option<OptionPath> {
        let line = self.line_content.trim();

        // Common patterns:
        // - "services.nginx.enable = true;"
        // - "config.services.nginx.enable"
        // - "options.services.nginx.enable = mkOption { ... };"

        // Find the path expression around the cursor position
        // Simple heuristic: look for dotted identifier paths

        // Strip common prefixes
        let line = line
            .trim_start_matches("config.")
            .trim_start_matches("options.");

        // Extract the path part (up to '=' or end)
        let path_str = if let Some(eq_pos) = line.find('=') {
            line[..eq_pos].trim()
        } else {
            line.trim_end_matches(';')
        };

        // Split by dots and build path
        let components: Vec<_> = path_str
            .split('.')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .filter(|s| s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
            .map(|s| s.to_string())
            .collect();

        if components.is_empty() {
            None
        } else {
            Some(OptionPath::new(components))
        }
    }
}

/// Hover provider
pub struct HoverProvider {
    capabilities: Capabilities,
    registry: OptionRegistry,
}

impl HoverProvider {
    /// Create a new hover provider
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            capabilities,
            registry: OptionRegistry::new(),
        }
    }

    /// Create with an option registry
    pub fn with_registry(capabilities: Capabilities, registry: OptionRegistry) -> Self {
        Self { capabilities, registry }
    }

    /// Update the option registry
    pub fn set_registry(&mut self, registry: OptionRegistry) {
        self.registry = registry;
    }

    /// Get the option registry
    pub fn registry(&self) -> &OptionRegistry {
        &self.registry
    }

    /// Check if hover is enabled
    pub fn is_enabled(&self) -> bool {
        self.capabilities.hover
    }

    /// Get hover info for a path
    pub fn hover(&self, path: &OptionPath) -> Option<HoverInfo> {
        if !self.is_enabled() {
            return None;
        }

        self.registry.get(path).map(|info| HoverInfo {
            path: info.path.clone(),
            type_desc: info.type_desc.clone(),
            default: info.default.as_ref().map(format_value),
            description: info.description.clone(),
            declared_in: info.declared_in.clone(),
            example: None,
        })
    }

    /// Get hover info from context
    pub fn hover_at(&self, context: &HoverContext) -> Option<HoverInfo> {
        let path = context.extract_path()?;
        self.hover(&path)
    }

    /// Get hover info with fuzzy matching
    pub fn hover_fuzzy(&self, path: &OptionPath) -> Option<HoverInfo> {
        // First try exact match
        if let Some(info) = self.hover(path) {
            return Some(info);
        }

        // Try to find the closest ancestor that exists
        let components = path.components();
        for i in (0..components.len()).rev() {
            let ancestor_path = OptionPath::new(components[..i].to_vec());
            if let Some(info) = self.hover(&ancestor_path) {
                return Some(info);
            }
        }

        None
    }
}

/// Format a value for display
fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("\"{}\"", s),
        Value::Path(p) => p.display().to_string(),
        Value::List(items) => {
            if items.is_empty() {
                "[ ]".to_string()
            } else if items.len() <= 3 {
                let formatted: Vec<_> = items.iter().map(format_value).collect();
                format!("[ {} ]", formatted.join(", "))
            } else {
                format!("[ ... ] ({} items)", items.len())
            }
        }
        Value::Attrs(attrs) => {
            if attrs.is_empty() {
                "{ }".to_string()
            } else if attrs.len() <= 3 {
                let formatted: Vec<_> = attrs
                    .iter()
                    .map(|(k, v)| format!("{} = {}", k, format_value(v)))
                    .collect();
                format!("{{ {} }}", formatted.join("; "))
            } else {
                format!("{{ ... }} ({} attrs)", attrs.len())
            }
        }
        Value::Lambda => "<function>".to_string(),
        Value::Derivation(inner) => format!("<derivation: {}>", format_value(inner)),
    }
}

/// Convert HoverInfo to LSP Hover
#[cfg(feature = "lsp")]
impl HoverInfo {
    /// Convert to lsp_types::Hover
    pub fn to_lsp_hover(&self) -> lsp_types::Hover {
        use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

        Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: self.to_markdown(),
            }),
            range: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::OptionInfo;
    use crate::types::Value;
    use std::path::PathBuf;

    fn make_registry() -> OptionRegistry {
        let mut registry = OptionRegistry::new();

        registry.register(OptionInfo {
            path: OptionPath::from_dotted("services.nginx.enable"),
            type_desc: "bool".to_string(),
            default: Some(Value::Bool(false)),
            description: Some("Whether to enable nginx web server.".to_string()),
            declared_in: vec![PathBuf::from("nixos/modules/services/web-servers/nginx/default.nix")],
            internal: false,
        });

        registry.register(OptionInfo {
            path: OptionPath::from_dotted("services.nginx.port"),
            type_desc: "int".to_string(),
            default: Some(Value::Int(80)),
            description: Some("Port to listen on.".to_string()),
            declared_in: vec![],
            internal: false,
        });

        registry
    }

    #[test]
    fn test_hover_markdown() {
        let info = HoverInfo {
            path: OptionPath::new(vec!["services".to_string(), "nginx".to_string(), "enable".to_string()]),
            type_desc: "boolean".to_string(),
            default: Some("false".to_string()),
            description: Some("Whether to enable nginx web server.".to_string()),
            declared_in: vec![PathBuf::from("nixos/modules/services/web-servers/nginx/default.nix")],
            example: None,
        };

        let md = info.to_markdown();
        assert!(md.contains("services.nginx.enable"));
        assert!(md.contains("boolean"));
        assert!(md.contains("false"));
        assert!(md.contains("nginx web server"));
    }

    #[test]
    fn test_hover_provider() {
        let caps = Capabilities::all();
        let provider = HoverProvider::with_registry(caps, make_registry());

        let path = OptionPath::from_dotted("services.nginx.enable");
        let info = provider.hover(&path).unwrap();

        assert_eq!(info.type_desc, "bool");
        assert_eq!(info.default, Some("false".to_string()));
        assert!(info.description.as_ref().unwrap().contains("nginx"));
    }

    #[test]
    fn test_hover_not_found() {
        let caps = Capabilities::all();
        let provider = HoverProvider::with_registry(caps, make_registry());

        let path = OptionPath::from_dotted("nonexistent.option");
        assert!(provider.hover(&path).is_none());
    }

    #[test]
    fn test_hover_context() {
        let ctx = HoverContext::new(0, 0)
            .with_line_content("services.nginx.enable = true;".to_string());

        let path = ctx.extract_path().unwrap();
        assert_eq!(path.to_dotted(), "services.nginx.enable");
    }

    #[test]
    fn test_format_value() {
        assert_eq!(format_value(&Value::Bool(true)), "true");
        assert_eq!(format_value(&Value::Int(42)), "42");
        assert_eq!(format_value(&Value::String("test".into())), "\"test\"");
        assert_eq!(format_value(&Value::List(vec![])), "[ ]");
    }
}
