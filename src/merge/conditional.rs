//! Conditional evaluation for mkIf and mkMerge.
//!
//! This module provides helper functions for detecting and evaluating
//! conditional definitions wrapped in mkIf and mkMerge.

use crate::types::Value;
use indexmap::IndexMap;

/// Check if a value is an mkIf wrapper.
///
/// An mkIf value is an attribute set with:
/// - `_type` == "if"
/// - `condition` - a boolean value
/// - `content` - the wrapped value
pub fn is_mk_if(value: &Value) -> bool {
    match value {
        Value::Attrs(attrs) => {
            matches!(attrs.get("_type"), Some(Value::String(s)) if s == "if")
        }
        _ => false,
    }
}

/// Check if a value is an mkMerge wrapper.
///
/// An mkMerge value is an attribute set with:
/// - `_type` == "merge"
/// - `contents` - a list of values to merge
pub fn is_mk_merge(value: &Value) -> bool {
    match value {
        Value::Attrs(attrs) => {
            matches!(attrs.get("_type"), Some(Value::String(s)) if s == "merge")
        }
        _ => false,
    }
}

/// Extract the condition from an mkIf value.
///
/// Returns `Some(true)` if condition is true, `Some(false)` if false,
/// or `None` if the value is not a valid mkIf or condition is not a bool.
pub fn extract_condition(value: &Value) -> Option<bool> {
    match value {
        Value::Attrs(attrs) => {
            // Verify this is an mkIf
            if !matches!(attrs.get("_type"), Some(Value::String(s)) if s == "if") {
                return None;
            }

            // Extract the condition
            match attrs.get("condition") {
                Some(Value::Bool(b)) => Some(*b),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Extract the content from an mkIf value.
///
/// Returns the wrapped content if this is a valid mkIf, or `None` otherwise.
pub fn extract_content(value: &Value) -> Option<Value> {
    match value {
        Value::Attrs(attrs) => {
            // Verify this is an mkIf
            if !matches!(attrs.get("_type"), Some(Value::String(s)) if s == "if") {
                return None;
            }

            // Extract the content
            attrs.get("content").cloned()
        }
        _ => None,
    }
}

/// Extract the contents list from an mkMerge value.
///
/// Returns the list of values to merge, or `None` if not a valid mkMerge.
pub fn extract_merge_contents(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Attrs(attrs) => {
            // Verify this is an mkMerge
            if !matches!(attrs.get("_type"), Some(Value::String(s)) if s == "merge") {
                return None;
            }

            // Extract the contents
            match attrs.get("contents") {
                Some(Value::List(list)) => Some(list.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Process a single value, evaluating any mkIf conditions.
///
/// This recursively handles:
/// - mkIf: evaluates condition and returns content if true, None if false
/// - mkMerge: flattens and recursively processes each item
/// - Regular values: returns as-is wrapped in Some
///
/// Returns a vector of active values after condition evaluation.
pub fn process_conditional(value: Value) -> Vec<Value> {
    if is_mk_if(&value) {
        // Handle mkIf
        match extract_condition(&value) {
            Some(true) => {
                // Condition is true - process the content (which may be nested mkIf/mkMerge)
                if let Some(content) = extract_content(&value) {
                    process_conditional(content)
                } else {
                    vec![]
                }
            }
            Some(false) => {
                // Condition is false - exclude this definition
                vec![]
            }
            None => {
                // Invalid mkIf (non-bool condition) - pass through as-is
                // This allows for lazy evaluation where condition might be evaluated later
                vec![value]
            }
        }
    } else if is_mk_merge(&value) {
        // Handle mkMerge - flatten and process each item
        if let Some(contents) = extract_merge_contents(&value) {
            contents
                .into_iter()
                .flat_map(process_conditional)
                .collect()
        } else {
            // Invalid mkMerge - pass through as-is
            vec![value]
        }
    } else {
        // Regular value - pass through
        vec![value]
    }
}

/// Create an mkIf value (useful for testing)
pub fn mk_if(condition: bool, content: Value) -> Value {
    let mut attrs = IndexMap::new();
    attrs.insert("_type".to_string(), Value::String("if".to_string()));
    attrs.insert("condition".to_string(), Value::Bool(condition));
    attrs.insert("content".to_string(), content);
    Value::Attrs(attrs)
}

/// Create an mkMerge value (useful for testing)
pub fn mk_merge(contents: Vec<Value>) -> Value {
    let mut attrs = IndexMap::new();
    attrs.insert("_type".to_string(), Value::String("merge".to_string()));
    attrs.insert("contents".to_string(), Value::List(contents));
    Value::Attrs(attrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_mk_if() {
        let mk_if_val = mk_if(true, Value::String("hello".into()));
        assert!(is_mk_if(&mk_if_val));

        let not_mk_if = Value::String("hello".into());
        assert!(!is_mk_if(&not_mk_if));

        let wrong_type = {
            let mut attrs = IndexMap::new();
            attrs.insert("_type".to_string(), Value::String("other".to_string()));
            Value::Attrs(attrs)
        };
        assert!(!is_mk_if(&wrong_type));
    }

    #[test]
    fn test_is_mk_merge() {
        let mk_merge_val = mk_merge(vec![Value::String("a".into())]);
        assert!(is_mk_merge(&mk_merge_val));

        let not_mk_merge = Value::String("hello".into());
        assert!(!is_mk_merge(&not_mk_merge));

        let mk_if_val = mk_if(true, Value::String("hello".into()));
        assert!(!is_mk_merge(&mk_if_val));
    }

    #[test]
    fn test_extract_condition() {
        let mk_if_true = mk_if(true, Value::String("hello".into()));
        assert_eq!(extract_condition(&mk_if_true), Some(true));

        let mk_if_false = mk_if(false, Value::String("hello".into()));
        assert_eq!(extract_condition(&mk_if_false), Some(false));

        let not_mk_if = Value::String("hello".into());
        assert_eq!(extract_condition(&not_mk_if), None);

        // Invalid mkIf with non-bool condition
        let invalid_mk_if = {
            let mut attrs = IndexMap::new();
            attrs.insert("_type".to_string(), Value::String("if".to_string()));
            attrs.insert("condition".to_string(), Value::String("not a bool".into()));
            attrs.insert("content".to_string(), Value::String("hello".into()));
            Value::Attrs(attrs)
        };
        assert_eq!(extract_condition(&invalid_mk_if), None);
    }

    #[test]
    fn test_extract_content() {
        let content = Value::String("hello".into());
        let mk_if_val = mk_if(true, content.clone());
        assert_eq!(extract_content(&mk_if_val), Some(content));

        let not_mk_if = Value::String("hello".into());
        assert_eq!(extract_content(&not_mk_if), None);
    }

    #[test]
    fn test_simple_mk_if_true() {
        let content = Value::String("active".into());
        let mk_if_val = mk_if(true, content.clone());

        let result = process_conditional(mk_if_val);
        assert_eq!(result, vec![content]);
    }

    #[test]
    fn test_simple_mk_if_false() {
        let content = Value::String("inactive".into());
        let mk_if_val = mk_if(false, content);

        let result = process_conditional(mk_if_val);
        assert!(result.is_empty());
    }

    #[test]
    fn test_nested_mk_if() {
        // mkIf true (mkIf true "value") -> ["value"]
        let inner = mk_if(true, Value::String("value".into()));
        let outer = mk_if(true, inner);

        let result = process_conditional(outer);
        assert_eq!(result, vec![Value::String("value".into())]);

        // mkIf true (mkIf false "value") -> []
        let inner_false = mk_if(false, Value::String("value".into()));
        let outer_true = mk_if(true, inner_false);

        let result = process_conditional(outer_true);
        assert!(result.is_empty());

        // mkIf false (mkIf true "value") -> []
        let inner_true = mk_if(true, Value::String("value".into()));
        let outer_false = mk_if(false, inner_true);

        let result = process_conditional(outer_false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_mk_merge_containing_mk_if() {
        // mkMerge [
        //   (mkIf true "a")
        //   (mkIf false "b")
        //   "c"
        // ]
        // -> ["a", "c"]
        let merge_val = mk_merge(vec![
            mk_if(true, Value::String("a".into())),
            mk_if(false, Value::String("b".into())),
            Value::String("c".into()),
        ]);

        let result = process_conditional(merge_val);
        assert_eq!(
            result,
            vec![Value::String("a".into()), Value::String("c".into())]
        );
    }

    #[test]
    fn test_nested_mk_merge() {
        // mkMerge [
        //   mkMerge ["a", "b"]
        //   "c"
        // ]
        // -> ["a", "b", "c"]
        let inner_merge = mk_merge(vec![
            Value::String("a".into()),
            Value::String("b".into()),
        ]);
        let outer_merge = mk_merge(vec![inner_merge, Value::String("c".into())]);

        let result = process_conditional(outer_merge);
        assert_eq!(
            result,
            vec![
                Value::String("a".into()),
                Value::String("b".into()),
                Value::String("c".into())
            ]
        );
    }

    #[test]
    fn test_mk_if_containing_mk_merge() {
        // mkIf true (mkMerge ["a", "b"]) -> ["a", "b"]
        let inner_merge = mk_merge(vec![
            Value::String("a".into()),
            Value::String("b".into()),
        ]);
        let outer_if = mk_if(true, inner_merge);

        let result = process_conditional(outer_if);
        assert_eq!(
            result,
            vec![Value::String("a".into()), Value::String("b".into())]
        );

        // mkIf false (mkMerge ["a", "b"]) -> []
        let inner_merge = mk_merge(vec![
            Value::String("a".into()),
            Value::String("b".into()),
        ]);
        let outer_if_false = mk_if(false, inner_merge);

        let result = process_conditional(outer_if_false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_regular_value_passthrough() {
        let value = Value::String("hello".into());
        let result = process_conditional(value.clone());
        assert_eq!(result, vec![value]);

        let list_value = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let result = process_conditional(list_value.clone());
        assert_eq!(result, vec![list_value]);
    }

    #[test]
    fn test_complex_nested_structure() {
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

        let result = process_conditional(outer);
        assert_eq!(
            result,
            vec![Value::String("a".into()), Value::String("d".into())]
        );
    }
}
