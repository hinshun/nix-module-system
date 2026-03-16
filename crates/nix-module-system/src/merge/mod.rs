//! Merge engine for combining multiple option definitions.
//!
//! This module implements the core merging logic that combines
//! definitions from multiple modules into final configuration values.

mod conditional;
mod lattice;
mod priority;
mod strategy;

pub use conditional::*;
pub use lattice::*;
pub use priority::*;
pub use strategy::*;

use crate::errors::TypeError;
use crate::types::{Definition, MergeResult, NixType, OptionPath};

/// The merge engine that combines definitions
#[derive(Debug, Default)]
pub struct MergeEngine {
    /// Whether to collect all errors instead of failing fast
    collect_errors: bool,
    /// Collected errors (if collect_errors is true)
    errors: Vec<TypeError>,
}

impl MergeEngine {
    /// Create a new merge engine
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable error collection mode
    pub fn with_error_collection(mut self) -> Self {
        self.collect_errors = true;
        self
    }

    /// Merge definitions for a single option
    pub fn merge_option(
        &mut self,
        ty: &dyn NixType,
        path: &OptionPath,
        defs: Vec<Definition>,
    ) -> Result<MergeResult, TypeError> {
        // Process mkIf conditions
        let active_defs = self.filter_conditional(defs);

        // Apply priority filtering
        let priority_defs = filter_by_priority(active_defs);

        // Delegate to type's merge function
        ty.merge(path, priority_defs)
    }

    /// Filter definitions by mkIf conditions.
    ///
    /// This evaluates mkIf conditions and expands mkMerge wrappers,
    /// returning only definitions whose conditions are true.
    pub fn filter_conditional(&self, defs: Vec<Definition>) -> Vec<Definition> {
        defs.into_iter()
            .flat_map(|def| {
                // Process the value through conditional evaluation
                let processed_values = process_conditional(def.value);

                // Create new definitions for each resulting value,
                // preserving the original metadata (file, span, priority)
                processed_values.into_iter().map(move |value| Definition {
                    file: def.file.clone(),
                    span: def.span.clone(),
                    value,
                    priority: def.priority,
                })
            })
            .collect()
    }

    /// Get collected errors
    pub fn take_errors(&mut self) -> Vec<TypeError> {
        std::mem::take(&mut self.errors)
    }
}

/// Filter definitions by priority, keeping only highest priority ones
pub fn filter_by_priority(mut defs: Vec<Definition>) -> Vec<Definition> {
    if defs.is_empty() {
        return defs;
    }

    let min_priority = defs.iter().map(|d| d.priority).min().unwrap_or(100);

    defs.retain(|d| d.priority == min_priority);
    defs
}

/// Standard priority levels used in the module system
pub mod priorities {
    /// Default priority (normal definitions)
    pub const DEFAULT: i32 = 100;

    /// mkOptionDefault priority
    pub const OPTION_DEFAULT: i32 = 1500;

    /// mkDefault priority
    pub const MK_DEFAULT: i32 = 1000;

    /// mkForce priority
    pub const MK_FORCE: i32 = 50;

    /// mkOverride with custom priority
    pub fn mk_override(priority: i32) -> i32 {
        priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Str, Value};

    #[test]
    fn test_filter_by_priority() {
        let defs = vec![
            Definition::with_priority(Value::String("low".into()), 1000),
            Definition::with_priority(Value::String("high".into()), 50),
            Definition::with_priority(Value::String("high2".into()), 50),
        ];

        let filtered = filter_by_priority(defs);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|d| d.priority == 50));
    }

    #[test]
    fn test_merge_engine() {
        let mut engine = MergeEngine::new();
        let ty = Str;
        let path = OptionPath::root();

        let defs = vec![Definition::new(Value::String("hello".into()))];

        let result = engine.merge_option(&ty, &path, defs).unwrap();
        assert_eq!(result.value, Value::String("hello".into()));
    }

    #[test]
    fn test_filter_conditional_simple_mk_if_true() {
        let engine = MergeEngine::new();
        let content = Value::String("active".into());
        let mk_if_val = mk_if(true, content.clone());

        let defs = vec![Definition::new(mk_if_val)];
        let filtered = engine.filter_conditional(defs);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, content);
    }

    #[test]
    fn test_filter_conditional_simple_mk_if_false() {
        let engine = MergeEngine::new();
        let content = Value::String("inactive".into());
        let mk_if_val = mk_if(false, content);

        let defs = vec![Definition::new(mk_if_val)];
        let filtered = engine.filter_conditional(defs);

        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_conditional_nested_mk_if() {
        let engine = MergeEngine::new();

        // mkIf true (mkIf true "value") -> ["value"]
        let inner = mk_if(true, Value::String("value".into()));
        let outer = mk_if(true, inner);

        let defs = vec![Definition::new(outer)];
        let filtered = engine.filter_conditional(defs);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, Value::String("value".into()));

        // mkIf true (mkIf false "value") -> []
        let inner_false = mk_if(false, Value::String("value".into()));
        let outer_true = mk_if(true, inner_false);

        let defs = vec![Definition::new(outer_true)];
        let filtered = engine.filter_conditional(defs);

        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_conditional_mk_merge_containing_mk_if() {
        let engine = MergeEngine::new();

        // mkMerge [
        //   (mkIf true "a")
        //   (mkIf false "b")
        //   "c"
        // ]
        // -> definitions for ["a", "c"]
        let merge_val = mk_merge(vec![
            mk_if(true, Value::String("a".into())),
            mk_if(false, Value::String("b".into())),
            Value::String("c".into()),
        ]);

        let defs = vec![Definition::new(merge_val)];
        let filtered = engine.filter_conditional(defs);

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].value, Value::String("a".into()));
        assert_eq!(filtered[1].value, Value::String("c".into()));
    }

    #[test]
    fn test_filter_conditional_mixed_definitions() {
        let engine = MergeEngine::new();

        // Mix of regular definitions and mkIf definitions
        let defs = vec![
            Definition::new(Value::String("regular1".into())),
            Definition::new(mk_if(true, Value::String("conditional_true".into()))),
            Definition::new(mk_if(false, Value::String("conditional_false".into()))),
            Definition::new(Value::String("regular2".into())),
        ];

        let filtered = engine.filter_conditional(defs);

        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].value, Value::String("regular1".into()));
        assert_eq!(filtered[1].value, Value::String("conditional_true".into()));
        assert_eq!(filtered[2].value, Value::String("regular2".into()));
    }

    #[test]
    fn test_filter_conditional_preserves_priority() {
        let engine = MergeEngine::new();

        // mkIf with priority should preserve the priority
        let mk_if_val = mk_if(true, Value::String("forced".into()));
        let defs = vec![Definition::with_priority(mk_if_val, 50)]; // mkForce priority

        let filtered = engine.filter_conditional(defs);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, Value::String("forced".into()));
        assert_eq!(filtered[0].priority, 50);
    }

    #[test]
    fn test_merge_engine_with_mk_if_true() {
        let mut engine = MergeEngine::new();
        let ty = Str;
        let path = OptionPath::root();

        let mk_if_val = mk_if(true, Value::String("conditional".into()));
        let defs = vec![Definition::new(mk_if_val)];

        let result = engine.merge_option(&ty, &path, defs).unwrap();
        assert_eq!(result.value, Value::String("conditional".into()));
    }

    #[test]
    fn test_merge_engine_with_mk_if_false_fallback() {
        let mut engine = MergeEngine::new();
        let ty = Str;
        let path = OptionPath::root();

        // When mkIf is false, use the other definition
        let defs = vec![
            Definition::new(mk_if(false, Value::String("conditional".into()))),
            Definition::new(Value::String("fallback".into())),
        ];

        let result = engine.merge_option(&ty, &path, defs).unwrap();
        assert_eq!(result.value, Value::String("fallback".into()));
    }

    #[test]
    fn test_merge_engine_with_mk_merge() {
        let mut engine = MergeEngine::new();
        let ty = Str;
        let path = OptionPath::root();

        // mkMerge with all same values should work
        let merge_val = mk_merge(vec![
            Value::String("same".into()),
            mk_if(true, Value::String("same".into())),
        ]);

        let defs = vec![Definition::new(merge_val)];

        let result = engine.merge_option(&ty, &path, defs).unwrap();
        assert_eq!(result.value, Value::String("same".into()));
    }

    #[test]
    fn test_filter_conditional_complex_nested() {
        let engine = MergeEngine::new();

        // mkMerge [
        //   (mkIf true (mkMerge [
        //     (mkIf true "a")
        //     (mkIf false "b")
        //   ]))
        //   (mkIf false "c")
        //   "d"
        // ]
        // -> ["a", "d"]
        let deep_merge = mk_merge(vec![
            mk_if(true, Value::String("a".into())),
            mk_if(false, Value::String("b".into())),
        ]);
        let outer = mk_merge(vec![
            mk_if(true, deep_merge),
            mk_if(false, Value::String("c".into())),
            Value::String("d".into()),
        ]);

        let defs = vec![Definition::new(outer)];
        let filtered = engine.filter_conditional(defs);

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].value, Value::String("a".into()));
        assert_eq!(filtered[1].value, Value::String("d".into()));
    }

    #[test]
    fn test_filter_conditional_multiple_definitions_with_mk_merge() {
        let engine = MergeEngine::new();

        // Multiple definitions, some with mkMerge
        let defs = vec![
            Definition::new(Value::String("first".into())),
            Definition::new(mk_merge(vec![
                mk_if(true, Value::String("second".into())),
                mk_if(false, Value::String("excluded".into())),
            ])),
            Definition::new(mk_if(true, Value::String("third".into()))),
        ];

        let filtered = engine.filter_conditional(defs);

        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].value, Value::String("first".into()));
        assert_eq!(filtered[1].value, Value::String("second".into()));
        assert_eq!(filtered[2].value, Value::String("third".into()));
    }
}
