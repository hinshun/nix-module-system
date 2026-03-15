//! Module collection - resolves imports and builds the module list.
//!
//! This module handles the collection phase of module evaluation:
//! - Parsing module files to extract imports
//! - Resolving relative paths
//! - Building the complete module list with deduplication
//! - Topological sorting based on dependencies

use crate::errors::EvalError;
use crate::parse::{self, AttrName, Binding, Expr, Spanned};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// A collected module with resolved imports
#[derive(Debug, Clone)]
pub struct CollectedModule {
    /// Path to the module file
    pub file: PathBuf,
    /// Unique key for deduplication
    pub key: String,
    /// Direct imports (already resolved to paths)
    pub imports: Vec<PathBuf>,
    /// Whether this module is disabled
    pub disabled: bool,
    /// The parsed AST
    pub ast: Option<Spanned<Expr>>,
    /// Extracted options (if any)
    pub options: Option<Spanned<Expr>>,
    /// Extracted config (if any)
    pub config: Option<Spanned<Expr>>,
    /// Whether this is a function module
    pub is_function: bool,
}

impl CollectedModule {
    /// Create a new module from a file path
    pub fn new(file: PathBuf) -> Self {
        Self {
            key: file.display().to_string(),
            file,
            imports: Vec::new(),
            disabled: false,
            ast: None,
            options: None,
            config: None,
            is_function: false,
        }
    }

    /// Create from a parsed AST
    pub fn from_ast(file: PathBuf, ast: Spanned<Expr>) -> Self {
        let mut module = Self::new(file);
        module.analyze_module(&ast);
        module.ast = Some(ast);
        module
    }

    /// Analyze a module AST to extract imports, options, and config
    fn analyze_module(&mut self, ast: &Spanned<Expr>) {
        match &ast.node {
            Expr::Lambda(lambda) => {
                self.is_function = true;
                // Analyze the body of the lambda
                self.analyze_module_body(&lambda.body);
            }
            Expr::AttrSet(attrs) => {
                self.analyze_attrset_body(&attrs.bindings);
            }
            _ => {}
        }
    }

    /// Analyze the body of a module (either lambda body or direct attrset)
    fn analyze_module_body(&mut self, body: &Spanned<Expr>) {
        match &body.node {
            Expr::AttrSet(attrs) => {
                self.analyze_attrset_body(&attrs.bindings);
            }
            _ => {}
        }
    }

    /// Analyze attribute set bindings to extract imports, options, config
    fn analyze_attrset_body(&mut self, bindings: &[Binding]) {
        for binding in bindings {
            if let Binding::Simple { path, value } = binding {
                if let Some(name) = get_simple_attr_name(path) {
                    match name.as_str() {
                        "imports" => {
                            self.imports = extract_import_paths(value);
                        }
                        "options" => {
                            self.options = Some(value.clone());
                        }
                        "config" => {
                            self.config = Some(value.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Get a simple attribute name from a path (if single element)
fn get_simple_attr_name(path: &[Spanned<AttrName>]) -> Option<String> {
    if path.len() == 1 {
        if let AttrName::Ident(name) = &path[0].node {
            return Some(name.clone());
        }
    }
    None
}

/// Extract import paths from an expression
fn extract_import_paths(expr: &Spanned<Expr>) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    match &expr.node {
        Expr::List(elements) => {
            for elem in elements {
                match &elem.node {
                    Expr::Path(p) => {
                        paths.push(p.clone());
                    }
                    Expr::Apply { func: _, arg } => {
                        // Handle things like: ./module.nix or (import ./module.nix)
                        if let Expr::Path(p) = &arg.node {
                            paths.push(p.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        Expr::Path(p) => {
            paths.push(p.clone());
        }
        _ => {}
    }

    paths
}

/// Module collector that handles import resolution
pub struct ModuleCollector {
    /// Root directory for resolving relative paths
    root_dir: PathBuf,
    /// Already collected modules (by key)
    collected: HashMap<String, CollectedModule>,
    /// Collection order
    order: Vec<String>,
    /// Disabled module keys
    disabled: HashSet<String>,
    /// Parse errors accumulated
    errors: Vec<(PathBuf, String)>,
}

impl ModuleCollector {
    /// Create a new module collector
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            collected: HashMap::new(),
            order: Vec::new(),
            disabled: HashSet::new(),
            errors: Vec::new(),
        }
    }

    /// Set disabled modules
    pub fn with_disabled(mut self, disabled: HashSet<String>) -> Self {
        self.disabled = disabled;
        self
    }

    /// Collect all modules starting from the given roots
    pub fn collect(&mut self, roots: Vec<PathBuf>) -> Result<Vec<CollectedModule>, EvalError> {
        // Process each root module
        for root in roots {
            self.collect_module(root)?;
        }

        // Return modules in collection order
        let mut result = Vec::new();
        for key in &self.order {
            if let Some(module) = self.collected.remove(key) {
                result.push(module);
            }
        }

        Ok(result)
    }

    /// Collect a single module and its imports
    fn collect_module(&mut self, path: PathBuf) -> Result<(), EvalError> {
        // Resolve to absolute path
        let abs_path = self.resolve_path(&path)?;
        let key = abs_path.display().to_string();

        // Skip if already collected
        if self.collected.contains_key(&key) {
            return Ok(());
        }

        // Check if disabled
        let disabled = self.disabled.contains(&key)
            || self.disabled.contains(abs_path.file_name().unwrap_or_default().to_str().unwrap_or(""));

        // Parse the module file
        let module = self.parse_module(&abs_path, disabled)?;

        // Get imports before inserting (to avoid borrow issues)
        let imports = module.imports.clone();

        // Insert into collected
        self.collected.insert(key.clone(), module);
        self.order.push(key);

        // Recursively collect imports
        for import_path in imports {
            let resolved = self.resolve_import(&abs_path, &import_path)?;
            self.collect_module(resolved)?;
        }

        Ok(())
    }

    /// Parse a module file
    fn parse_module(&mut self, path: &PathBuf, disabled: bool) -> Result<CollectedModule, EvalError> {
        // Read the file
        let source = std::fs::read_to_string(path).map_err(|e| {
            EvalError::Io {
                path: path.clone(),
                message: format!("Failed to read module file: {}", e),
            }
        })?;

        // Parse the source
        match parse::parse_module(&source, path.clone()) {
            Ok(ast) => {
                let mut module = CollectedModule::from_ast(path.clone(), ast);
                module.disabled = disabled;
                Ok(module)
            }
            Err(errors) => {
                // Collect parse errors but continue
                for err in errors {
                    self.errors.push((path.clone(), err.message));
                }

                // Return a module without AST
                let mut module = CollectedModule::new(path.clone());
                module.disabled = disabled;
                Ok(module)
            }
        }
    }

    /// Resolve a path relative to root or current directory
    fn resolve_path(&self, path: &PathBuf) -> Result<PathBuf, EvalError> {
        if path.is_absolute() {
            return Ok(path.clone());
        }

        // Try relative to root dir
        let resolved = self.root_dir.join(path);
        if resolved.exists() {
            return Ok(resolved.canonicalize().map_err(|e| {
                EvalError::Io {
                    path: resolved.clone(),
                    message: format!("Failed to canonicalize path: {}", e),
                }
            })?);
        }

        // Path doesn't exist
        Err(EvalError::ModuleNotFound {
            path: path.clone(),
            search_paths: vec![self.root_dir.clone()],
        })
    }

    /// Resolve an import path relative to the importing module
    fn resolve_import(&self, from_module: &PathBuf, import_path: &PathBuf) -> Result<PathBuf, EvalError> {
        if import_path.is_absolute() {
            return Ok(import_path.clone());
        }

        // Resolve relative to the importing module's directory
        let module_dir = from_module.parent().unwrap_or(&self.root_dir);
        let resolved = module_dir.join(import_path);

        if resolved.exists() {
            return resolved.canonicalize().map_err(|e| {
                EvalError::Io {
                    path: resolved.clone(),
                    message: format!("Failed to canonicalize import path: {}", e),
                }
            });
        }

        // Check if adding .nix extension helps
        let with_ext = resolved.with_extension("nix");
        if with_ext.exists() {
            return with_ext.canonicalize().map_err(|e| {
                EvalError::Io {
                    path: with_ext.clone(),
                    message: format!("Failed to canonicalize import path: {}", e),
                }
            });
        }

        // Check for default.nix in directory
        let default_nix = resolved.join("default.nix");
        if default_nix.exists() {
            return default_nix.canonicalize().map_err(|e| {
                EvalError::Io {
                    path: default_nix.clone(),
                    message: format!("Failed to canonicalize import path: {}", e),
                }
            });
        }

        Err(EvalError::ImportNotFound {
            import_path: import_path.clone(),
            from_module: from_module.clone(),
        })
    }

    /// Get accumulated parse errors
    pub fn errors(&self) -> &[(PathBuf, String)] {
        &self.errors
    }
}

/// Collect all modules starting from the given roots
pub fn collect_modules(
    roots: Vec<PathBuf>,
    disabled: HashSet<String>,
) -> Result<Vec<CollectedModule>, EvalError> {
    let root_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut collector = ModuleCollector::new(root_dir).with_disabled(disabled);
    collector.collect(roots)
}

/// Filter out disabled modules
pub fn filter_disabled(modules: Vec<CollectedModule>) -> Vec<CollectedModule> {
    modules.into_iter().filter(|m| !m.disabled).collect()
}

/// Build a dependency graph from collected modules
pub fn build_dependency_graph(modules: &[CollectedModule]) -> IndexMap<String, Vec<String>> {
    let mut graph: IndexMap<String, Vec<String>> = IndexMap::new();

    for module in modules {
        let deps: Vec<String> = module.imports
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        graph.insert(module.key.clone(), deps);
    }

    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collected_module_new() {
        let module = CollectedModule::new(PathBuf::from("test.nix"));
        assert_eq!(module.key, "test.nix");
        assert!(!module.disabled);
        assert!(!module.is_function);
    }

    #[test]
    fn test_filter_disabled() {
        let mut m1 = CollectedModule::new(PathBuf::from("a.nix"));
        m1.disabled = false;

        let mut m2 = CollectedModule::new(PathBuf::from("b.nix"));
        m2.disabled = true;

        let filtered = filter_disabled(vec![m1, m2]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key, "a.nix");
    }

    #[test]
    fn test_build_dependency_graph() {
        let mut m1 = CollectedModule::new(PathBuf::from("a.nix"));
        m1.imports = vec![PathBuf::from("b.nix")];

        let m2 = CollectedModule::new(PathBuf::from("b.nix"));

        let graph = build_dependency_graph(&[m1, m2]);

        assert!(graph.contains_key("a.nix"));
        assert_eq!(graph["a.nix"], vec!["b.nix"]);
        assert!(graph.contains_key("b.nix"));
        assert!(graph["b.nix"].is_empty());
    }
}
