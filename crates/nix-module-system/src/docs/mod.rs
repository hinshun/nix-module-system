//! Auto-generated reference documentation for module options.
//!
//! This module provides high-quality documentation generation inspired by
//! Unison's documentation system, featuring:
//!
//! - Type-aware option documentation
//! - Hierarchical navigation
//! - Cross-references between related options
//! - Multiple output formats (Markdown, HTML, JSON)

mod render;

pub use render::*;

use crate::types::{NixType, OptionDoc, OptionPath, Value};
use indexmap::IndexMap;
use std::collections::HashMap;

/// Documentation tree for a module system
#[derive(Debug, Clone)]
pub struct DocTree {
    /// Root options
    pub options: IndexMap<OptionPath, OptionEntry>,
    /// Module metadata
    pub metadata: DocMetadata,
}

/// Metadata for documentation
#[derive(Debug, Clone, Default)]
pub struct DocMetadata {
    /// Module system name
    pub name: String,
    /// Version
    pub version: String,
    /// Description
    pub description: String,
    /// Authors
    pub authors: Vec<String>,
}

/// A documentation entry for an option
#[derive(Debug, Clone)]
pub struct OptionEntry {
    /// Option documentation
    pub doc: OptionDoc,
    /// Children options (for nested structures)
    pub children: Vec<OptionPath>,
    /// Related options (cross-references)
    pub related: Vec<OptionPath>,
    /// Example configurations
    pub examples: Vec<Example>,
}

/// An example configuration
#[derive(Debug, Clone)]
pub struct Example {
    /// Example title
    pub title: String,
    /// Example description
    pub description: Option<String>,
    /// The example value
    pub value: Value,
}

impl DocTree {
    /// Create a new documentation tree
    pub fn new() -> Self {
        Self {
            options: IndexMap::new(),
            metadata: DocMetadata::default(),
        }
    }

    /// Build documentation from a type
    pub fn from_type(ty: &dyn NixType) -> Self {
        let mut tree = Self::new();
        tree.add_type_options(ty, &OptionPath::root());
        tree
    }

    /// Add options from a type
    fn add_type_options(&mut self, ty: &dyn NixType, prefix: &OptionPath) {
        let sub_options = ty.get_sub_options(prefix);

        for (path, doc) in sub_options {
            let entry = OptionEntry {
                doc,
                children: Vec::new(),
                related: Vec::new(),
                examples: Vec::new(),
            };
            self.options.insert(path, entry);
        }

        // Build parent-child relationships
        self.build_hierarchy();
    }

    /// Build the parent-child hierarchy
    fn build_hierarchy(&mut self) {
        let paths: Vec<OptionPath> = self.options.keys().cloned().collect();

        for path in &paths {
            let components = path.components();
            if components.len() > 1 {
                // Find parent path
                let parent_components = &components[..components.len() - 1];
                let parent_path = OptionPath::new(parent_components.to_vec());

                if let Some(parent) = self.options.get_mut(&parent_path) {
                    if !parent.children.contains(path) {
                        parent.children.push(path.clone());
                    }
                }
            }
        }
    }

    /// Find related options based on naming patterns
    pub fn find_related(&self, path: &OptionPath) -> Vec<OptionPath> {
        let components = path.components();
        let mut related = Vec::new();

        // Find siblings (options with same parent)
        if components.len() > 1 {
            let parent_prefix = &components[..components.len() - 1];
            for (opt_path, _) in &self.options {
                let opt_components = opt_path.components();
                if opt_components.len() == components.len()
                    && opt_components[..opt_components.len() - 1] == *parent_prefix
                    && opt_path != path
                {
                    related.push(opt_path.clone());
                }
            }
        }

        // Find options with similar names
        if let Some(last) = components.last() {
            for (opt_path, _) in &self.options {
                if opt_path != path {
                    if let Some(opt_last) = opt_path.components().last() {
                        if opt_last == last {
                            related.push(opt_path.clone());
                        }
                    }
                }
            }
        }

        related
    }

    /// Get all top-level options
    pub fn top_level(&self) -> Vec<&OptionPath> {
        self.options
            .keys()
            .filter(|p| p.components().len() == 1)
            .collect()
    }

    /// Get options at a specific depth
    pub fn at_depth(&self, depth: usize) -> Vec<&OptionPath> {
        self.options
            .keys()
            .filter(|p| p.components().len() == depth)
            .collect()
    }
}

impl Default for DocTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Search results
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Matching option path
    pub path: OptionPath,
    /// Match score (higher is better)
    pub score: f64,
    /// Highlighted match context
    pub context: String,
}

/// Documentation search engine
pub struct DocSearch {
    /// Index of option paths
    path_index: HashMap<String, Vec<OptionPath>>,
    /// Index of descriptions
    description_index: HashMap<String, Vec<OptionPath>>,
}

impl DocSearch {
    /// Build a search index from a doc tree
    pub fn build(tree: &DocTree) -> Self {
        let mut path_index: HashMap<String, Vec<OptionPath>> = HashMap::new();
        let mut description_index: HashMap<String, Vec<OptionPath>> = HashMap::new();

        for (path, entry) in &tree.options {
            // Index path components
            for component in path.components() {
                path_index
                    .entry(component.to_lowercase())
                    .or_default()
                    .push(path.clone());
            }

            // Index description words
            if let Some(desc) = &entry.doc.description {
                for word in desc.split_whitespace() {
                    let word = word
                        .trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase();
                    if word.len() >= 3 {
                        description_index
                            .entry(word)
                            .or_default()
                            .push(path.clone());
                    }
                }
            }
        }

        Self {
            path_index,
            description_index,
        }
    }

    /// Search for options matching a query
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        let mut scores: HashMap<OptionPath, f64> = HashMap::new();

        // Search path index (higher weight)
        for (term, paths) in &self.path_index {
            if term.contains(&query_lower) {
                let score = if term == &query_lower {
                    10.0
                } else if term.starts_with(&query_lower) {
                    5.0
                } else {
                    2.0
                };

                for path in paths {
                    *scores.entry(path.clone()).or_default() += score;
                }
            }
        }

        // Search description index (lower weight)
        for (term, paths) in &self.description_index {
            if term.contains(&query_lower) {
                let score = if term == &query_lower { 3.0 } else { 1.0 };

                for path in paths {
                    *scores.entry(path.clone()).or_default() += score;
                }
            }
        }

        // Sort by score
        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        results
            .into_iter()
            .map(|(path, score)| SearchResult {
                context: format!("{}", path),
                path,
                score,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Bool, Module, OptionDecl, Str, Submodule};

    #[test]
    fn test_doc_tree_from_type() {
        let module = Module::new()
            .with_option(
                "enable",
                OptionDecl::new(Box::new(Bool))
                    .with_description("Enable the service")
                    .with_default(Value::Bool(false)),
            )
            .with_option(
                "name",
                OptionDecl::new(Box::new(Str)).with_description("Service name"),
            );

        let ty = Submodule::new(vec![module]);
        let tree = DocTree::from_type(&ty);

        assert!(tree.options.contains_key(&OptionPath::new(vec!["enable".to_string()])));
        assert!(tree.options.contains_key(&OptionPath::new(vec!["name".to_string()])));
    }

    #[test]
    fn test_search() {
        let module = Module::new()
            .with_option(
                "enable",
                OptionDecl::new(Box::new(Bool)).with_description("Enable the nginx web server"),
            )
            .with_option(
                "package",
                OptionDecl::new(Box::new(Str)).with_description("The nginx package to use"),
            );

        let ty = Submodule::new(vec![module]);
        let tree = DocTree::from_type(&ty);
        let search = DocSearch::build(&tree);

        let results = search.search("enable", 10);
        assert!(!results.is_empty());
        assert!(results[0].path.to_dotted().contains("enable"));
    }
}
