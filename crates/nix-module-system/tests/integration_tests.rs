//! Integration tests for the Nix module system.
//!
//! These tests verify the core module system logic: type checking, merging,
//! conditional evaluation, and priority handling. Actual Nix parsing and
//! evaluation is handled by the Nix evaluator.

use nix_module_system::types::{
    AttrsOf, Bool, Definition, Int, ListOf, NixType, OptionPath, Str, Value,
};
use nix_module_system::merge::{mk_if, mk_merge, process_conditional, MergeEngine};
use nix_module_system::errors::TypeError;
use indexmap::IndexMap;

// ============================================================================
// Type System Tests
// ============================================================================

#[test]
fn test_bool_type_check() {
    let ty = Bool;
    assert!(ty.check(&Value::Bool(true)).is_ok());
    assert!(ty.check(&Value::Bool(false)).is_ok());
    assert!(ty.check(&Value::String("true".into())).is_err());
}

#[test]
fn test_list_of_type() {
    let ty = ListOf::new(Box::new(Int));

    // Valid list
    let valid = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    assert!(ty.check(&valid).is_ok());

    // ListOf uses shallow type checking - it verifies the value is a list
    // but element type validation happens during merge, not check
    let mixed = Value::List(vec![Value::Int(1), Value::String("two".into())]);
    assert!(ty.check(&mixed).is_ok()); // Shallow check passes

    // Non-list fails
    assert!(ty.check(&Value::Int(42)).is_err());
}

#[test]
fn test_attrs_of_type() {
    let ty = AttrsOf::new(Box::new(Str));

    let mut attrs = IndexMap::new();
    attrs.insert("key1".to_string(), Value::String("value1".into()));
    attrs.insert("key2".to_string(), Value::String("value2".into()));

    assert!(ty.check(&Value::Attrs(attrs)).is_ok());
}

#[test]
fn test_list_merge() {
    let ty = ListOf::new(Box::new(Int));
    let path = OptionPath::root();

    let defs = vec![
        Definition::new(Value::List(vec![Value::Int(1), Value::Int(2)])),
        Definition::new(Value::List(vec![Value::Int(3)])),
    ];

    let result = ty.merge(&path, defs).unwrap();

    if let Value::List(items) = result.value {
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::Int(1));
        assert_eq!(items[1], Value::Int(2));
        assert_eq!(items[2], Value::Int(3));
    } else {
        panic!("Expected list");
    }
}

#[test]
fn test_attrs_merge() {
    let ty = AttrsOf::new(Box::new(Int));
    let path = OptionPath::root();

    let mut attrs1 = IndexMap::new();
    attrs1.insert("a".to_string(), Value::Int(1));

    let mut attrs2 = IndexMap::new();
    attrs2.insert("b".to_string(), Value::Int(2));

    let defs = vec![
        Definition::new(Value::Attrs(attrs1)),
        Definition::new(Value::Attrs(attrs2)),
    ];

    let result = ty.merge(&path, defs).unwrap();

    if let Value::Attrs(attrs) = result.value {
        assert_eq!(attrs.get("a"), Some(&Value::Int(1)));
        assert_eq!(attrs.get("b"), Some(&Value::Int(2)));
    } else {
        panic!("Expected attrs");
    }
}

// ============================================================================
// Conditional Evaluation Tests
// ============================================================================

#[test]
fn test_mkif_true_evaluates() {
    let value = mk_if(true, Value::String("enabled".into()));
    let result = process_conditional(value);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0], Value::String("enabled".into()));
}

#[test]
fn test_mkif_false_filtered() {
    let value = mk_if(false, Value::String("disabled".into()));
    let result = process_conditional(value);

    assert!(result.is_empty());
}

#[test]
fn test_mkmerge_combines() {
    let value = mk_merge(vec![
        Value::String("a".into()),
        Value::String("b".into()),
        Value::String("c".into()),
    ]);

    let result = process_conditional(value);

    assert_eq!(result.len(), 3);
}

#[test]
fn test_nested_conditionals() {
    let nested_merge = mk_merge(vec![
        mk_if(true, Value::String("nested-included".into())),
    ]);

    let value = mk_merge(vec![
        mk_if(true, Value::String("included".into())),
        mk_if(false, Value::String("excluded".into())),
        nested_merge,
    ]);

    let result = process_conditional(value);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0], Value::String("included".into()));
    assert_eq!(result[1], Value::String("nested-included".into()));
}

// ============================================================================
// Merge Engine Tests
// ============================================================================

#[test]
fn test_merge_engine_basic() {
    let mut engine = MergeEngine::new();
    let ty = Str;
    let path = OptionPath::new(vec!["test".into()]);

    let defs = vec![Definition::new(Value::String("hello".into()))];

    let result = engine.merge_option(&ty, &path, defs).unwrap();
    assert_eq!(result.value, Value::String("hello".into()));
}

#[test]
fn test_merge_engine_with_priority() {
    let mut engine = MergeEngine::new();
    let ty = Str;
    let path = OptionPath::root();

    let defs = vec![
        Definition::with_priority(Value::String("default".into()), 1000),
        Definition::with_priority(Value::String("forced".into()), 50),
        Definition::with_priority(Value::String("normal".into()), 100),
    ];

    let result = engine.merge_option(&ty, &path, defs).unwrap();
    assert_eq!(result.value, Value::String("forced".into()));
}

#[test]
fn test_merge_engine_conditional_filtering() {
    let mut engine = MergeEngine::new();
    let ty = Str;
    let path = OptionPath::root();

    let defs = vec![
        Definition::new(mk_if(false, Value::String("skipped".into()))),
        Definition::new(mk_if(true, Value::String("included".into()))),
    ];

    let result = engine.merge_option(&ty, &path, defs).unwrap();
    assert_eq!(result.value, Value::String("included".into()));
}

// ============================================================================
// Option Path Tests
// ============================================================================

#[test]
fn test_option_path_creation() {
    let path = OptionPath::new(vec!["services".into(), "nginx".into(), "enable".into()]);
    assert_eq!(path.to_dotted(), "services.nginx.enable");
}

#[test]
fn test_option_path_child() {
    let parent = OptionPath::new(vec!["services".into()]);
    let child = parent.child("nginx");
    assert_eq!(child.to_dotted(), "services.nginx");
}

#[test]
fn test_option_path_from_string() {
    let path = OptionPath::from_dotted("services.nginx.enable");
    assert_eq!(path.components(), &["services", "nginx", "enable"]);
}

// ============================================================================
// Value Manipulation Tests
// ============================================================================

#[test]
fn test_value_display() {
    assert_eq!(format!("{}", Value::Null), "null");
    assert_eq!(format!("{}", Value::Bool(true)), "true");
    assert_eq!(format!("{}", Value::Int(42)), "42");
    assert_eq!(format!("{}", Value::String("hello".into())), "\"hello\"");
}

#[test]
fn test_value_list() {
    let list = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);

    if let Value::List(items) = list {
        assert_eq!(items.len(), 3);
    }
}

#[test]
fn test_value_attrs() {
    let mut attrs = IndexMap::new();
    attrs.insert("key".to_string(), Value::String("value".into()));

    let value = Value::Attrs(attrs);

    if let Value::Attrs(a) = value {
        assert_eq!(a.get("key"), Some(&Value::String("value".into())));
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_type_mismatch_error() {
    let err = TypeError::Mismatch {
        expected: "bool".to_string(),
        found: "string".to_string(),
        value: Some(Value::String("test".into())),
    };

    let msg = format!("{}", err);
    assert!(msg.contains("expected"));
    assert!(msg.contains("bool"));
}

#[test]
fn test_undefined_option_error() {
    let err = TypeError::UndefinedOption {
        path: OptionPath::new(vec!["invalid".into()]),
        available: vec!["valid1".into(), "valid2".into()],
    };

    let msg = format!("{}", err);
    assert!(msg.contains("undefined"));
}

// ============================================================================
// Full Evaluation Scenario Tests
// ============================================================================

#[test]
fn test_module_evaluation_scenario() {
    let mut config_attrs = IndexMap::new();

    let mut services = IndexMap::new();
    let mut test = IndexMap::new();
    test.insert("enable".to_string(), Value::Bool(true));
    services.insert("test".to_string(), Value::Attrs(test));
    config_attrs.insert("services".to_string(), Value::Attrs(services));

    let mut users = IndexMap::new();
    let mut users_inner = IndexMap::new();
    let mut test_user = IndexMap::new();
    test_user.insert("isSystemUser".to_string(), Value::Bool(true));
    users_inner.insert("test".to_string(), Value::Attrs(test_user));
    users.insert("users".to_string(), Value::Attrs(users_inner));
    config_attrs.insert("users".to_string(), Value::Attrs(users));

    let config = Value::Attrs(config_attrs);

    if let Value::Attrs(ref attrs) = config {
        assert!(attrs.contains_key("services"));
        assert!(attrs.contains_key("users"));

        if let Some(Value::Attrs(services)) = attrs.get("services") {
            if let Some(Value::Attrs(test)) = services.get("test") {
                assert_eq!(test.get("enable"), Some(&Value::Bool(true)));
            }
        }
    }
}

#[test]
fn test_merge_scenario_with_conditionals() {
    let condition = true;

    let mut base = IndexMap::new();
    base.insert("base".to_string(), Value::Bool(true));

    let mut conditional = IndexMap::new();
    conditional.insert("conditional".to_string(), Value::Bool(true));

    let merged = mk_merge(vec![
        Value::Attrs(base),
        mk_if(condition, Value::Attrs(conditional)),
    ]);

    let results = process_conditional(merged);

    assert_eq!(results.len(), 2);
}

#[test]
fn test_priority_override_scenario() {
    let mut engine = MergeEngine::new();
    let ty = Int;
    let path = OptionPath::new(vec!["some".into(), "option".into()]);

    let defs = vec![
        Definition::with_priority(Value::Int(100), 100),
        Definition::with_priority(Value::Int(200), 1000),
        Definition::with_priority(Value::Int(42), 50),
    ];

    let result = engine.merge_option(&ty, &path, defs).unwrap();
    assert_eq!(result.value, Value::Int(42));
}
