//! Type system for the Nix module system.
//!
//! This module provides Rust implementations of Nix types for high-performance
//! type checking and value merging.
//!
//! ## Type Hierarchy
//!
//! - Primitive types: `str`, `bool`, `int`, `float`, `path`
//! - Compound types: `listOf`, `attrsOf`, `nullOr`, `oneOf`
//! - Module types: `submodule`, `deferredModule`
//!
//! ## Merge Strategies
//!
//! Each type defines how multiple definitions are merged:
//! - `str`: Must be equal or use priority
//! - `bool`: Must be equal, or use `boolByOr` for OR merging
//! - `listOf`: Concatenate all definitions
//! - `attrsOf`: Recursive merge of attributes

mod base;
mod compound;
mod submodule;
mod traits;

pub use base::*;
pub use compound::*;
pub use submodule::*;
pub use traits::*;

// Note: TypeError and TypeResult are used by submodules via pub use
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// A Nix value that can be type-checked and merged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value (64-bit signed)
    Int(i64),
    /// Float value (64-bit)
    Float(f64),
    /// String value
    String(String),
    /// Path value
    Path(PathBuf),
    /// List of values
    List(Vec<Value>),
    /// Attribute set
    Attrs(IndexMap<String, Value>),
    /// Lambda/function (cannot be serialized)
    #[serde(skip)]
    Lambda,
    /// Derivation (special attribute set)
    Derivation(Box<Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "\"{}\"", s.escape_default()),
            Value::Path(p) => write!(f, "{}", p.display()),
            Value::List(l) => {
                write!(f, "[ ")?;
                for (i, v) in l.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, " ]")
            }
            Value::Attrs(a) => {
                write!(f, "{{ ")?;
                for (k, v) in a.iter() {
                    write!(f, "{} = {}; ", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Lambda => write!(f, "<lambda>"),
            Value::Derivation(_) => write!(f, "<derivation>"),
        }
    }
}

/// Location in source code for error reporting.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    /// Source file path
    pub file: PathBuf,
    /// Start byte offset
    pub start: usize,
    /// End byte offset
    pub end: usize,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (1-indexed)
    pub column: usize,
}

impl Span {
    /// Create a new span
    pub fn new(file: PathBuf, start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            file,
            start,
            end,
            line,
            column,
        }
    }

    /// Get the byte range for ariadne
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

/// A definition of an option value with metadata.
#[derive(Debug, Clone)]
pub struct Definition {
    /// Source file
    pub file: PathBuf,
    /// Location in file
    pub span: Option<Span>,
    /// The defined value
    pub value: Value,
    /// Priority (lower = higher priority, default 100)
    pub priority: i32,
}

impl Definition {
    /// Create a new definition with default priority
    pub fn new(value: Value) -> Self {
        Self {
            file: PathBuf::new(),
            span: None,
            value,
            priority: 100,
        }
    }

    /// Create a definition with a specific priority
    pub fn with_priority(value: Value, priority: i32) -> Self {
        Self {
            file: PathBuf::new(),
            span: None,
            value,
            priority,
        }
    }
}

/// Path to an option in the configuration tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OptionPath(Vec<String>);

impl OptionPath {
    /// Create the root path
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// Create a path from components
    pub fn new(components: Vec<String>) -> Self {
        Self(components)
    }

    /// Append a component to the path
    pub fn push(&mut self, component: String) {
        self.0.push(component);
    }

    /// Create a child path
    pub fn child(&self, component: &str) -> Self {
        let mut new = self.0.clone();
        new.push(component.to_string());
        Self(new)
    }

    /// Extend this path with additional components
    pub fn extend(&self, components: &[String]) -> Self {
        let mut new = self.0.clone();
        new.extend(components.iter().cloned());
        Self(new)
    }

    /// Get path components
    pub fn components(&self) -> &[String] {
        &self.0
    }

    /// Check if this is the root path
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Format as dotted string (e.g., "services.nginx.enable")
    pub fn to_dotted(&self) -> String {
        self.0.join(".")
    }

    /// Parse from dotted string (e.g., "services.nginx.enable")
    pub fn from_dotted(s: &str) -> Self {
        if s.is_empty() {
            Self::root()
        } else {
            Self(s.split('.').map(|s| s.to_string()).collect())
        }
    }
}

impl fmt::Display for OptionPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_dotted())
    }
}

/// Result of merging definitions
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The merged value
    pub value: Value,
    /// Whether a default was used
    pub used_default: bool,
    /// The winning definition (if priority-based)
    pub winning_def: Option<Definition>,
}

impl MergeResult {
    /// Create a simple merge result
    pub fn new(value: Value) -> Self {
        Self {
            value,
            used_default: false,
            winning_def: None,
        }
    }

    /// Create a result using a default value
    pub fn from_default(value: Value) -> Self {
        Self {
            value,
            used_default: true,
            winning_def: None,
        }
    }
}

/// Registry of available types
#[derive(Default)]
pub struct TypeRegistry {
    types: IndexMap<String, Box<dyn NixType>>,
}

impl TypeRegistry {
    /// Create a new registry with built-in types
    pub fn new() -> Self {
        let mut registry = Self::default();

        // Register primitive types
        registry.register("str", Box::new(Str));
        registry.register("bool", Box::new(Bool));
        registry.register("int", Box::new(Int));
        registry.register("float", Box::new(Float));
        registry.register("path", Box::new(Path));

        registry
    }

    /// Register a type
    pub fn register(&mut self, name: &str, ty: Box<dyn NixType>) {
        self.types.insert(name.to_string(), ty);
    }

    /// Get a type by name
    pub fn get(&self, name: &str) -> Option<&dyn NixType> {
        self.types.get(name).map(|t| t.as_ref())
    }

    /// Create a listOf type
    pub fn list_of(&self, elem_type: &str) -> Option<Box<dyn NixType>> {
        let elem = self.get(elem_type)?;
        Some(Box::new(ListOf::new(elem.clone_box())))
    }

    /// Create an attrsOf type
    pub fn attrs_of(&self, elem_type: &str) -> Option<Box<dyn NixType>> {
        let elem = self.get(elem_type)?;
        Some(Box::new(AttrsOf::new(elem.clone_box())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_display() {
        assert_eq!(Value::Null.to_string(), "null");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::String("hello".into()).to_string(), "\"hello\"");
    }

    #[test]
    fn test_option_path() {
        let path = OptionPath::new(vec!["services".into(), "nginx".into(), "enable".into()]);
        assert_eq!(path.to_dotted(), "services.nginx.enable");

        let child = path.child("foo");
        assert_eq!(child.to_dotted(), "services.nginx.enable.foo");
    }

    #[test]
    fn test_type_registry() {
        let registry = TypeRegistry::new();
        assert!(registry.get("str").is_some());
        assert!(registry.get("bool").is_some());
        assert!(registry.get("nonexistent").is_none());
    }
}
