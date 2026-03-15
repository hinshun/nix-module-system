//! Core traits for the type system.

use super::{Definition, MergeResult, OptionPath, Value};
use crate::errors::{TypeError, TypeResult};
use std::collections::HashMap;
use std::fmt::Debug;

/// Core trait that all Nix types must implement.
///
/// This trait defines the behavior for type checking and merging values.
/// Implementations should be thread-safe (Send + Sync) for parallel evaluation.
pub trait NixType: Send + Sync + Debug {
    /// Get the type name for error messages.
    fn name(&self) -> &str;

    /// Get a human-readable description of the type.
    fn description(&self) -> String {
        self.name().to_string()
    }

    /// Check if a value matches this type.
    ///
    /// This should only check the root of the value - nested values
    /// are checked during merge to maintain laziness.
    fn check(&self, value: &Value) -> TypeResult<()>;

    /// Merge multiple definitions into a single value.
    ///
    /// The `loc` parameter provides the option path for error messages.
    /// Definitions are provided in order of declaration.
    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError>;

    /// Get nested types for documentation.
    ///
    /// For compound types like `listOf` or `attrsOf`, this returns
    /// the element type. For `submodule`, this returns option types.
    fn nested_types(&self) -> HashMap<String, Box<dyn NixType>> {
        HashMap::new()
    }

    /// Get sub-options for documentation (submodule types).
    fn get_sub_options(&self, _prefix: &OptionPath) -> HashMap<OptionPath, OptionDoc> {
        HashMap::new()
    }

    /// Clone this type into a boxed trait object.
    fn clone_box(&self) -> Box<dyn NixType>;

    /// Check if this type has an empty value (for mkIf false).
    fn empty_value(&self) -> Option<Value> {
        None
    }

    /// Check if two values can be unified.
    fn can_unify(&self, a: &Value, b: &Value) -> bool {
        // Default: values must be equal
        a == b
    }

    /// Unify two values (lattice meet operation).
    fn unify(&self, a: &Value, b: &Value) -> Option<Value> {
        if self.can_unify(a, b) {
            Some(a.clone())
        } else {
            None
        }
    }
}

/// Documentation for an option (used by submodule types).
#[derive(Debug, Clone)]
pub struct OptionDoc {
    /// Option path
    pub path: OptionPath,
    /// Type description
    pub type_desc: String,
    /// Default value if any
    pub default: Option<Value>,
    /// Example value if any
    pub example: Option<Value>,
    /// Description text
    pub description: Option<String>,
    /// Whether the option is internal
    pub internal: bool,
    /// Whether the option is visible in docs
    pub visible: bool,
    /// Whether the option is read-only
    pub read_only: bool,
}

/// Trait for types that can be composed (listOf, attrsOf, etc.)
pub trait ComposableType: NixType {
    /// Get the element type
    fn element_type(&self) -> &dyn NixType;

    /// Create a new instance with a different element type
    fn with_element(&self, elem: Box<dyn NixType>) -> Box<dyn NixType>;
}

/// Trait for types that support option merging
pub trait MergeableType: NixType {
    /// Get the merge strategy name
    fn merge_strategy(&self) -> &str;

    /// Check if this type supports mkMerge
    fn supports_merge(&self) -> bool {
        true
    }

    /// Check if this type supports mkOverride
    fn supports_override(&self) -> bool {
        true
    }
}

/// Helper trait for priority-based merging
pub trait PriorityMerge: NixType {
    /// Filter definitions by priority and return only the highest priority ones.
    fn filter_by_priority(&self, defs: Vec<Definition>) -> Vec<Definition> {
        if defs.is_empty() {
            return defs;
        }

        let min_priority = defs.iter().map(|d| d.priority).min().unwrap_or(100);

        defs.into_iter()
            .filter(|d| d.priority == min_priority)
            .collect()
    }
}

// Implement PriorityMerge for all NixType
impl<T: NixType + ?Sized> PriorityMerge for T {}

/// Macro to implement Clone for boxed trait objects
#[macro_export]
macro_rules! impl_nix_type_clone {
    ($type:ty) => {
        impl Clone for Box<$type> {
            fn clone(&self) -> Self {
                self.clone_box()
            }
        }
    };
}

impl Clone for Box<dyn NixType> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Str;

    #[test]
    fn test_priority_filter() {
        let defs = vec![
            Definition::with_priority(Value::String("low".into()), 1000),
            Definition::with_priority(Value::String("high".into()), 50),
            Definition::with_priority(Value::String("high2".into()), 50),
        ];

        let ty = Str;
        let filtered = ty.filter_by_priority(defs);

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|d| d.priority == 50));
    }
}
