//! Option registry for LSP features.
//!
//! Maintains a searchable index of option declarations for completions and hover.

use crate::eval::OptionInfo;
use crate::types::OptionPath;
use indexmap::IndexMap;
use std::collections::HashMap;

/// Registry of option declarations for LSP features
#[derive(Debug, Default)]
pub struct OptionRegistry {
    /// All options indexed by path
    options: IndexMap<OptionPath, OptionInfo>,
    /// Prefix index for completion lookup
    prefix_index: HashMap<String, Vec<OptionPath>>,
}

impl OptionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry from evaluated options
    pub fn from_options(options: IndexMap<OptionPath, OptionInfo>) -> Self {
        let mut registry = Self {
            options,
            prefix_index: HashMap::new(),
        };
        registry.rebuild_index();
        registry
    }

    /// Rebuild the prefix index
    fn rebuild_index(&mut self) {
        self.prefix_index.clear();

        for path in self.options.keys() {
            // Index by each prefix
            let components = path.components();
            for i in 0..components.len() {
                let prefix = components[..=i].join(".");
                self.prefix_index
                    .entry(prefix)
                    .or_default()
                    .push(path.clone());
            }

            // Also index the empty prefix for root completions
            self.prefix_index
                .entry(String::new())
                .or_default()
                .push(path.clone());
        }
    }

    /// Register an option
    pub fn register(&mut self, info: OptionInfo) {
        let path = info.path.clone();

        // Update prefix index
        let components = path.components();
        for i in 0..components.len() {
            let prefix = components[..=i].join(".");
            self.prefix_index
                .entry(prefix)
                .or_default()
                .push(path.clone());
        }
        self.prefix_index
            .entry(String::new())
            .or_default()
            .push(path.clone());

        self.options.insert(path, info);
    }

    /// Look up an option by path
    pub fn get(&self, path: &OptionPath) -> Option<&OptionInfo> {
        self.options.get(path)
    }

    /// Get all options
    pub fn all_options(&self) -> &IndexMap<OptionPath, OptionInfo> {
        &self.options
    }

    /// Find options with a given prefix
    pub fn find_by_prefix(&self, prefix: &str) -> Vec<&OptionInfo> {
        self.prefix_index
            .get(prefix)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|p| self.options.get(p))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find child options of a given path (one level deep)
    pub fn find_children(&self, parent: &OptionPath) -> Vec<&OptionInfo> {
        let parent_depth = parent.components().len();
        let parent_str = parent.to_dotted();

        self.options
            .values()
            .filter(|info| {
                let components = info.path.components();
                if components.len() != parent_depth + 1 {
                    return false;
                }
                if parent.is_root() {
                    return true;
                }
                info.path.to_dotted().starts_with(&parent_str)
                    && info.path.to_dotted()[parent_str.len()..].starts_with('.')
            })
            .collect()
    }

    /// Find all options that start with a path prefix
    pub fn find_descendants(&self, parent: &OptionPath) -> Vec<&OptionInfo> {
        if parent.is_root() {
            return self.options.values().collect();
        }

        let parent_str = parent.to_dotted();
        self.options
            .values()
            .filter(|info| {
                let path_str = info.path.to_dotted();
                path_str.starts_with(&parent_str)
                    && (path_str.len() == parent_str.len()
                        || path_str[parent_str.len()..].starts_with('.'))
            })
            .collect()
    }

    /// Search options by name pattern (simple substring match)
    pub fn search(&self, query: &str) -> Vec<&OptionInfo> {
        let query_lower = query.to_lowercase();
        self.options
            .values()
            .filter(|info| {
                // Match against path
                info.path.to_dotted().to_lowercase().contains(&query_lower)
                    // Or description
                    || info.description.as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .collect()
    }

    /// Check if an option exists
    pub fn contains(&self, path: &OptionPath) -> bool {
        self.options.contains_key(path)
    }

    /// Get the number of registered options
    pub fn len(&self) -> usize {
        self.options.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.options.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Value;

    fn make_option(path: &str) -> OptionInfo {
        OptionInfo {
            path: OptionPath::from_dotted(path),
            type_desc: "bool".to_string(),
            default: Some(Value::Bool(false)),
            description: Some(format!("Option for {}", path)),
            declared_in: vec![],
            internal: false,
        }
    }

    #[test]
    fn test_registry_basic() {
        let mut registry = OptionRegistry::new();
        registry.register(make_option("services.nginx.enable"));
        registry.register(make_option("services.nginx.port"));
        registry.register(make_option("services.redis.enable"));

        assert_eq!(registry.len(), 3);
        assert!(registry.contains(&OptionPath::from_dotted("services.nginx.enable")));
    }

    #[test]
    fn test_find_children() {
        let mut registry = OptionRegistry::new();
        registry.register(make_option("services.nginx.enable"));
        registry.register(make_option("services.nginx.port"));
        registry.register(make_option("services.redis.enable"));
        registry.register(make_option("users.users.test"));

        let services_path = OptionPath::from_dotted("services");
        let children = registry.find_children(&services_path);
        // services has nginx and redis as direct children in paths
        // But those are not in registry - only services.nginx.enable etc.
        assert!(children.is_empty()); // No direct children at depth 1

        let nginx_path = OptionPath::from_dotted("services.nginx");
        let nginx_children = registry.find_children(&nginx_path);
        assert_eq!(nginx_children.len(), 2); // enable and port
    }

    #[test]
    fn test_find_descendants() {
        let mut registry = OptionRegistry::new();
        registry.register(make_option("services.nginx.enable"));
        registry.register(make_option("services.nginx.port"));
        registry.register(make_option("services.redis.enable"));

        let services_path = OptionPath::from_dotted("services");
        let descendants = registry.find_descendants(&services_path);
        assert_eq!(descendants.len(), 3);
    }

    #[test]
    fn test_search() {
        let mut registry = OptionRegistry::new();
        registry.register(make_option("services.nginx.enable"));
        registry.register(make_option("services.redis.enable"));
        registry.register(make_option("users.users.test"));

        let results = registry.search("nginx");
        assert_eq!(results.len(), 1);

        let results = registry.search("enable");
        assert_eq!(results.len(), 2);
    }
}
