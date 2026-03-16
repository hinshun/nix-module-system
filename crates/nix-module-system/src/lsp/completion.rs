//! LSP completion provider.
//!
//! Provides code completions for module options, config values, and imports.

use super::{Capabilities, OptionCompletion, OptionRegistry};
use crate::types::OptionPath;

/// Kind of completion trigger
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionTrigger {
    /// User invoked completion manually
    Invoked,
    /// Triggered by a character (e.g., '.')
    TriggerChar(char),
    /// Triggered for incomplete completion
    Incomplete,
}

/// Completion context
#[derive(Debug, Clone)]
pub struct CompletionContext {
    /// Position in the document (line)
    pub line: u32,
    /// Column position
    pub column: u32,
    /// Current path being completed (if any)
    pub path: Option<OptionPath>,
    /// Text before cursor on the current line
    pub line_prefix: String,
    /// What triggered the completion
    pub trigger: CompletionTrigger,
}

impl CompletionContext {
    /// Create a new completion context
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            line,
            column,
            path: None,
            line_prefix: String::new(),
            trigger: CompletionTrigger::Invoked,
        }
    }

    /// Set the current path
    pub fn with_path(mut self, path: OptionPath) -> Self {
        self.path = Some(path);
        self
    }

    /// Set the line prefix
    pub fn with_line_prefix(mut self, prefix: String) -> Self {
        self.line_prefix = prefix;
        self
    }

    /// Set the trigger
    pub fn with_trigger(mut self, trigger: CompletionTrigger) -> Self {
        self.trigger = trigger;
        self
    }

    /// Parse the current path from line prefix (excluding partial identifier)
    pub fn parse_path(&self) -> Option<OptionPath> {
        if self.path.is_some() {
            return self.path.clone();
        }

        // Find the start of the identifier path
        let prefix = self.line_prefix.trim();

        // Look for patterns like "services.nginx." or "config.services."
        // Skip leading keywords
        let path_str = prefix
            .trim_start_matches("config.")
            .trim_start_matches("options.")
            .trim_end_matches(" =");

        if path_str.is_empty() {
            return Some(OptionPath::root());
        }

        // If there's a partial identifier (no trailing dot), exclude it from path
        // e.g., "services.nginx.en" -> path is "services.nginx", partial is "en"
        let path_str = if !path_str.ends_with('.') {
            if let Some(dot_pos) = path_str.rfind('.') {
                &path_str[..dot_pos]
            } else {
                // Single identifier with no dots - return root
                return Some(OptionPath::root());
            }
        } else {
            path_str.trim_end_matches('.')
        };

        if path_str.is_empty() {
            return Some(OptionPath::root());
        }

        Some(OptionPath::from_dotted(path_str))
    }

    /// Check if we're completing after a dot
    pub fn is_dot_completion(&self) -> bool {
        matches!(self.trigger, CompletionTrigger::TriggerChar('.'))
            || self.line_prefix.trim_end().ends_with('.')
    }

    /// Get the partial identifier being typed (for filtering)
    pub fn partial_ident(&self) -> Option<&str> {
        let prefix = self.line_prefix.trim();
        if prefix.is_empty() {
            return None;
        }

        // Find the last component after the last dot
        if let Some(dot_pos) = prefix.rfind('.') {
            let after_dot = &prefix[dot_pos + 1..];
            if !after_dot.is_empty() {
                return Some(after_dot);
            }
        } else if !prefix.contains(' ') && !prefix.contains('=') {
            // Single identifier being typed
            return Some(prefix);
        }

        None
    }
}

/// Completion provider
pub struct CompletionProvider {
    capabilities: Capabilities,
    registry: OptionRegistry,
}

impl CompletionProvider {
    /// Create a new completion provider
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

    /// Check if completions are enabled
    pub fn is_enabled(&self) -> bool {
        self.capabilities.completion
    }

    /// Get completions for a given context
    pub fn complete(&self, context: &CompletionContext) -> Vec<OptionCompletion> {
        if !self.is_enabled() {
            return Vec::new();
        }

        let mut completions = Vec::new();

        // Parse the path from context
        let path = context.parse_path().unwrap_or_else(OptionPath::root);

        // Get candidate options based on context
        let candidates = if context.is_dot_completion() {
            // After a dot, show children of the current path
            self.get_child_completions(&path)
        } else {
            // Show all descendants that match
            self.get_descendant_completions(&path)
        };

        // Filter by partial identifier if present
        let filter = context.partial_ident();

        for info in candidates {
            // Extract the completion name (the component after the parent path)
            let completion_name = if path.is_root() {
                info.path.components().first().map(|s| s.as_str())
            } else {
                let parent_len = path.components().len();
                info.path.components().get(parent_len).map(|s| s.as_str())
            };

            let Some(name) = completion_name else { continue };

            // Apply filter
            if let Some(filter) = filter {
                if !name.to_lowercase().starts_with(&filter.to_lowercase()) {
                    continue;
                }
            }

            completions.push(OptionCompletion {
                name: name.to_string(),
                path: info.path.clone(),
                type_desc: info.type_desc.clone(),
                description: info.description.clone(),
                default: info.default.as_ref().map(|v| format!("{}", v)),
            });
        }

        // Deduplicate by name (keep first occurrence for each unique name)
        let mut seen = std::collections::HashSet::new();
        completions.retain(|c| seen.insert(c.name.clone()));

        // Sort by name
        completions.sort_by(|a, b| a.name.cmp(&b.name));

        completions
    }

    /// Get completions for children of a path
    fn get_child_completions(&self, parent: &OptionPath) -> Vec<&crate::eval::OptionInfo> {
        // Find all options that are direct children or have children under this path
        let parent_depth = parent.components().len();

        self.registry
            .all_options()
            .values()
            .filter(|info| {
                let components = info.path.components();
                if components.len() <= parent_depth {
                    return false;
                }
                if parent.is_root() {
                    return true;
                }
                // Check if this path is under the parent
                let parent_components = parent.components();
                for (i, component) in parent_components.iter().enumerate() {
                    if components.get(i) != Some(component) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    /// Get completions for descendants of a path
    fn get_descendant_completions(&self, parent: &OptionPath) -> Vec<&crate::eval::OptionInfo> {
        self.registry.find_descendants(parent)
    }
}

/// Convert OptionCompletion to LSP CompletionItem
#[cfg(feature = "lsp")]
impl OptionCompletion {
    /// Convert to lsp_types::CompletionItem
    pub fn to_lsp_item(&self) -> lsp_types::CompletionItem {
        use lsp_types::{CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind};

        let mut detail = self.type_desc.clone();
        if let Some(ref default) = self.default {
            detail.push_str(&format!(" = {}", default));
        }

        let documentation = self.description.as_ref().map(|desc| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: desc.clone(),
            })
        });

        CompletionItem {
            label: self.name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(detail),
            documentation,
            insert_text: Some(self.name.clone()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::OptionInfo;
    use crate::types::Value;

    fn make_registry() -> OptionRegistry {
        let mut registry = OptionRegistry::new();

        registry.register(OptionInfo {
            path: OptionPath::from_dotted("services.nginx.enable"),
            type_desc: "bool".to_string(),
            default: Some(Value::Bool(false)),
            description: Some("Whether to enable nginx.".to_string()),
            declared_in: vec![],
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

        registry.register(OptionInfo {
            path: OptionPath::from_dotted("services.redis.enable"),
            type_desc: "bool".to_string(),
            default: Some(Value::Bool(false)),
            description: Some("Whether to enable redis.".to_string()),
            declared_in: vec![],
            internal: false,
        });

        registry.register(OptionInfo {
            path: OptionPath::from_dotted("users.users.test.name"),
            type_desc: "str".to_string(),
            default: None,
            description: Some("User name.".to_string()),
            declared_in: vec![],
            internal: false,
        });

        registry
    }

    #[test]
    fn test_completion_provider() {
        let caps = Capabilities::all();
        let provider = CompletionProvider::new(caps);
        assert!(provider.is_enabled());
    }

    #[test]
    fn test_root_completion() {
        let caps = Capabilities::all();
        let provider = CompletionProvider::with_registry(caps, make_registry());

        let ctx = CompletionContext::new(0, 0);
        let completions = provider.complete(&ctx);

        // Should have "services" and "users" as top-level completions
        let names: Vec<_> = completions.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"services"));
        assert!(names.contains(&"users"));
    }

    #[test]
    fn test_dot_completion() {
        let caps = Capabilities::all();
        let provider = CompletionProvider::with_registry(caps, make_registry());

        let ctx = CompletionContext::new(0, 0)
            .with_line_prefix("services.".to_string())
            .with_trigger(CompletionTrigger::TriggerChar('.'));

        let completions = provider.complete(&ctx);

        // Should have "nginx" and "redis"
        let names: Vec<_> = completions.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"nginx"));
        assert!(names.contains(&"redis"));
    }

    #[test]
    fn test_filtered_completion() {
        let caps = Capabilities::all();
        let provider = CompletionProvider::with_registry(caps, make_registry());

        let ctx = CompletionContext::new(0, 0)
            .with_line_prefix("services.nginx.en".to_string());

        let completions = provider.complete(&ctx);

        // Should only have "enable" filtered by "en" prefix
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].name, "enable");
    }

    #[test]
    fn test_parse_path() {
        // Trailing dot - path up to the dot
        let ctx = CompletionContext::new(0, 0)
            .with_line_prefix("services.nginx.".to_string());
        let path = ctx.parse_path().unwrap();
        assert_eq!(path.to_dotted(), "services.nginx");

        // Partial identifier - path excludes partial
        let ctx2 = CompletionContext::new(0, 0)
            .with_line_prefix("services.nginx.en".to_string());
        let path2 = ctx2.parse_path().unwrap();
        assert_eq!(path2.to_dotted(), "services.nginx");
    }

    #[test]
    fn test_partial_ident() {
        let ctx = CompletionContext::new(0, 0)
            .with_line_prefix("services.nginx.en".to_string());

        assert_eq!(ctx.partial_ident(), Some("en"));

        let ctx2 = CompletionContext::new(0, 0)
            .with_line_prefix("services.nginx.".to_string());

        assert_eq!(ctx2.partial_ident(), None);
    }
}
