//! Module evaluation engine.
//!
//! This module implements the staged evaluation pipeline:
//! 1. Parse - Extract module structure from Nix source
//! 2. Collect - Resolve imports and build dependency graph
//! 3. Declare - Process option declarations to build schema
//! 4. Define - Merge config definitions using the type system
//!
//! ## Usage
//!
//! ```ignore
//! use nix_module_system::eval::{eval_modules, collect_modules};
//!
//! // Evaluate modules from files
//! let result = eval_modules(vec![PathBuf::from("configuration.nix")])?;
//!
//! // Or use the pipeline directly for more control
//! let modules = collect_modules(roots, HashSet::new())?;
//! let result = Pipeline::new().with_modules(modules).run()?;
//! ```

mod collect;
mod pipeline;
mod topo;

pub use collect::*;
pub use pipeline::*;
pub use topo::*;

use crate::errors::EvalError;
use crate::types::{OptionPath, Value};
use indexmap::IndexMap;
use std::collections::HashSet;
use std::path::PathBuf;

/// Result of evaluating modules
#[derive(Debug)]
pub struct EvalResult {
    /// Final merged configuration
    pub config: Value,
    /// Option schema with types and metadata
    pub options: IndexMap<OptionPath, OptionInfo>,
    /// Warnings generated during evaluation
    pub warnings: Vec<String>,
}

impl EvalResult {
    /// Get a config value at a path
    pub fn get(&self, path: &OptionPath) -> Option<&Value> {
        get_value_at_path(&self.config, path)
    }

    /// Check if an option is defined
    pub fn is_defined(&self, path: &OptionPath) -> bool {
        self.get(path).is_some()
    }
}

/// Get a value at a path from a root value
fn get_value_at_path<'a>(value: &'a Value, path: &OptionPath) -> Option<&'a Value> {
    let components = path.components();
    if components.is_empty() {
        return Some(value);
    }

    match value {
        Value::Attrs(attrs) => {
            let first = &components[0];
            match attrs.get(first) {
                Some(v) if components.len() == 1 => Some(v),
                Some(v) => {
                    let rest = OptionPath::new(components[1..].to_vec());
                    get_value_at_path(v, &rest)
                }
                None => None,
            }
        }
        _ => None,
    }
}

/// Information about an option
#[derive(Debug, Clone)]
pub struct OptionInfo {
    /// Option path
    pub path: OptionPath,
    /// Type description
    pub type_desc: String,
    /// Default value
    pub default: Option<Value>,
    /// Description
    pub description: Option<String>,
    /// Where the option was declared
    pub declared_in: Vec<PathBuf>,
    /// Whether this is an internal option (hidden from documentation)
    pub internal: bool,
}

impl OptionInfo {
    /// Create new option info
    pub fn new(path: OptionPath) -> Self {
        Self {
            path,
            type_desc: "unspecified".to_string(),
            default: None,
            description: None,
            declared_in: Vec::new(),
            internal: false,
        }
    }

    /// Set type description
    pub fn with_type(mut self, type_desc: &str) -> Self {
        self.type_desc = type_desc.to_string();
        self
    }

    /// Set default value
    pub fn with_default(mut self, default: Value) -> Self {
        self.default = Some(default);
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Mark as internal option
    pub fn with_internal(mut self, internal: bool) -> Self {
        self.internal = internal;
        self
    }
}

/// Evaluate a list of modules.
///
/// This is the main entry point for module evaluation. It:
/// 1. Collects all modules starting from the given roots
/// 2. Resolves imports and builds the dependency graph
/// 3. Processes option declarations
/// 4. Merges config definitions
///
/// # Example
///
/// ```ignore
/// let result = eval_modules(vec![PathBuf::from("configuration.nix")])?;
/// println!("Config: {:?}", result.config);
/// ```
pub fn eval_modules(modules: Vec<PathBuf>) -> Result<EvalResult, EvalError> {
    // Collect modules with import resolution
    let collected = collect_modules(modules, HashSet::new())?;

    // Filter disabled modules
    let active = filter_disabled(collected);

    // Run the evaluation pipeline
    Pipeline::new().with_modules(active).run()
}

/// Evaluate modules with custom options.
pub fn eval_modules_with_options(
    modules: Vec<PathBuf>,
    disabled: HashSet<String>,
) -> Result<EvalResult, EvalError> {
    let collected = collect_modules(modules, disabled)?;
    let active = filter_disabled(collected);
    Pipeline::new().with_modules(active).run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_result_get() {
        let mut attrs = IndexMap::new();
        let mut nested = IndexMap::new();
        nested.insert("enable".to_string(), Value::Bool(true));
        attrs.insert("services".to_string(), Value::Attrs(nested));

        let result = EvalResult {
            config: Value::Attrs(attrs),
            options: IndexMap::new(),
            warnings: Vec::new(),
        };

        let path = OptionPath::new(vec!["services".into(), "enable".into()]);
        assert_eq!(result.get(&path), Some(&Value::Bool(true)));
        assert!(result.is_defined(&path));
    }

    #[test]
    fn test_option_info_builder() {
        let path = OptionPath::new(vec!["test".into()]);
        let info = OptionInfo::new(path)
            .with_type("bool")
            .with_default(Value::Bool(false))
            .with_description("Test option");

        assert_eq!(info.type_desc, "bool");
        assert_eq!(info.default, Some(Value::Bool(false)));
        assert_eq!(info.description, Some("Test option".to_string()));
    }
}
